//! Creating a new local store: the first-run tables, the owner-only file and
//! the first user with a fresh mnemonic. The legacy first run
//! ([`crate::db::connect`]) and `identity init` ([`create_store`]) share this
//! code, so both create the same store. It is the only part of `store` that
//! creates a file, and it never touches an existing store.
//!
//! Nothing here prints, and no error carries the mnemonic, a key or a path.

use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io;
use std::iter;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bip39::{Mnemonic, WordCount};
use nostr_sdk::prelude::PublicKey;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::db::User;
use crate::identity::identity_keys_from_mnemonic;
use crate::util::misc::ensure_private_dir;

/// The file name of the store inside the data directory.
pub const STORE_FILE: &str = "mcli.db";

/// The files SQLite keeps beside a store: a leftover one would be applied to,
/// or confused with, a new store of the same name.
const COMPANIONS: [&str; 3] = ["-journal", "-wal", "-shm"];

/// The tables of a new store, as the first run has always created them.
const SCHEMA: &str = r#"
          CREATE TABLE IF NOT EXISTS orders (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              status TEXT NOT NULL,
              amount INTEGER NOT NULL,
              min_amount INTEGER,
              max_amount INTEGER,
              fiat_code TEXT NOT NULL,
              fiat_amount INTEGER NOT NULL,
              payment_method TEXT NOT NULL,
              premium INTEGER NOT NULL,
              trade_keys TEXT,
              counterparty_pubkey TEXT,
              is_mine BOOLEAN,
              buyer_invoice TEXT,
              request_id INTEGER,
              created_at INTEGER,
              expires_at INTEGER,
              solver_pubkey TEXT
          );
          CREATE TABLE IF NOT EXISTS users (
              i0_pubkey char(64) PRIMARY KEY,
              mnemonic TEXT,
              last_trade_index INTEGER,
              created_at INTEGER
          );
          "#;

/// Create the SQLite database file restricted to the owner (mode `0600` on
/// Unix). On non-Unix platforms this falls back to a plain create.
///
/// Uses `create_new` so the file is created atomically with the right
/// permissions instead of being created world-readable and tightened
/// afterwards, which would leave a brief window where another local user could
/// open it. A second create never truncates an existing database.
pub fn create_private_db_file(path: impl AsRef<Path>) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path.as_ref()).map(|_| ())
}

/// Create the tables of a new store and its user, with a fresh 12-word
/// mnemonic and no trade index.
pub async fn initialize(pool: &SqlitePool) -> Result<User> {
    sqlx::query(SCHEMA).execute(pool).await?;
    let mnemonic = Mnemonic::generate(WordCount::Words12)?.to_string();
    User::new(mnemonic, pool).await
}

/// Why [`create_store`] created no store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateError {
    /// The store or one of its companion files already exists; nothing was
    /// touched.
    Exists,
    /// The directory or the file could not be created; nothing was written.
    NotCreated,
    /// The file was created but could not be completed; the file and any
    /// companion SQLite left beside it were removed again (the data directory,
    /// created or tightened by this run, stays).
    Failed,
}

/// Create the store in `dir` exactly as the first run does (the directory
/// owner-only, one level deep; the file owner-only; the tables; one user) and
/// return the identity public key. An existing store, or a companion file of
/// one, is left alone.
pub async fn create_store(dir: &Path) -> Result<PublicKey, CreateError> {
    create_with(dir, |path| async move { fill(&path).await }).await
}

/// [`create_store`] with the step that completes the created file given as
/// `fill`, which gets the path of the new, empty file and returns the identity
/// public key, or `None` when the file could not be completed.
async fn create_with<F, Fut>(dir: &Path, fill: F) -> Result<PublicKey, CreateError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Option<PublicKey>>,
{
    let path = dir.join(STORE_FILE);
    let companions: Vec<PathBuf> = COMPANIONS
        .iter()
        .map(|suffix| sibling(&path, suffix))
        .collect();
    for candidate in iter::once(&path).chain(&companions) {
        match fs::symlink_metadata(candidate) {
            Ok(_) => return Err(CreateError::Exists),
            Err(error) if is_absent(&error) => {}
            Err(_) => return Err(CreateError::NotCreated),
        }
    }
    let dir_text = dir.to_str().ok_or(CreateError::NotCreated)?;
    ensure_private_dir(dir_text).map_err(|_| CreateError::NotCreated)?;
    match create_private_db_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(CreateError::Exists)
        }
        Err(_) => return Err(CreateError::NotCreated),
    }
    match fill(path.clone()).await {
        Some(identity) => Ok(identity),
        None => {
            discard(&path, &companions);
            Err(CreateError::Failed)
        }
    }
}

/// Open the new, empty file, create the tables and the user, close it, and
/// return the identity public key derived the one way the program derives it.
async fn fill(path: &Path) -> Option<PublicKey> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;
    let user = initialize(&pool).await;
    pool.close().await;
    let keys = identity_keys_from_mnemonic(&user.ok()?.mnemonic).ok()?;
    Some(keys.public_key())
}

/// Remove the file this run created and any companion SQLite left beside it
/// (none existed before the run). Best effort.
fn discard(path: &Path, companions: &[PathBuf]) {
    let _ = fs::remove_file(path);
    for companion in companions {
        let _ = fs::remove_file(companion);
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Nothing is at the path: it does not exist, or its parent is not a
/// directory.
fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A unique directory under the system temp dir, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let seq = SEQ.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("mcli-create-{}-{seq}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn a_new_store_holds_one_user_whose_key_is_returned() {
        let dir = Dir::new();
        let key = create_store(&dir.0).await.unwrap();
        assert_eq!(dir.names(), [STORE_FILE]);
        let pool =
            SqlitePool::connect_with(SqliteConnectOptions::new().filename(dir.0.join(STORE_FILE)))
                .await
                .unwrap();
        let user = User::get(&pool).await.unwrap();
        pool.close().await;
        assert_eq!(user.i0_pubkey, key.to_hex());
        assert_eq!(user.last_trade_index, None);
    }

    #[tokio::test]
    async fn an_existing_store_or_companion_is_left_alone() {
        for name in ["mcli.db", "mcli.db-journal", "mcli.db-wal", "mcli.db-shm"] {
            let dir = Dir::new();
            fs::write(dir.0.join(name), b"keep me").unwrap();
            assert_eq!(
                create_store(&dir.0).await,
                Err(CreateError::Exists),
                "{name}"
            );
            assert_eq!(dir.names(), [name]);
            assert_eq!(fs::read(dir.0.join(name)).unwrap(), b"keep me");
        }
    }

    #[tokio::test]
    async fn a_directory_that_cannot_be_made_creates_nothing() {
        let dir = Dir::new();
        let orphan = dir.0.join("missing").join("mcli");
        assert_eq!(create_store(&orphan).await, Err(CreateError::NotCreated));
        let file = dir.0.join("file");
        fs::write(&file, b"x").unwrap();
        assert_eq!(create_store(&file).await, Err(CreateError::NotCreated));
        assert_eq!(dir.names(), ["file"]);
    }

    /// A fill that leaves a rollback journal beside the created file and fails.
    async fn fail_with_journal(path: PathBuf) -> Option<PublicKey> {
        assert_eq!(fs::read(&path).unwrap(), b"", "the created file is empty");
        fs::write(sibling(&path, "-journal"), b"journal").unwrap();
        None
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_completed_is_removed_and_failed() {
        let empty = Dir::new();
        assert_eq!(
            create_with(&empty.0, fail_with_journal).await,
            Err(CreateError::Failed)
        );
        assert_eq!(empty.names(), Vec::<String>::new());

        let holding = Dir::new();
        fs::write(holding.0.join("other"), b"kept").unwrap();
        assert_eq!(
            create_with(&holding.0, fail_with_journal).await,
            Err(CreateError::Failed)
        );
        assert_eq!(holding.names(), ["other"]);
        assert_eq!(fs::read(holding.0.join("other")).unwrap(), b"kept");
    }

    #[test]
    fn discarding_removes_the_store_and_its_companions_only() {
        let dir = Dir::new();
        let path = dir.0.join(STORE_FILE);
        let companions: Vec<PathBuf> = COMPANIONS.iter().map(|s| sibling(&path, s)).collect();
        fs::write(&path, b"half").unwrap();
        fs::write(&companions[0], b"journal").unwrap();
        fs::write(dir.0.join("other"), b"kept").unwrap();
        discard(&path, &companions);
        assert_eq!(dir.names(), ["other"]);
    }
}
