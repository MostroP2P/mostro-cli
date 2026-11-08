//! The config file `config` in the data directory (`MCLI_DIR`, else
//! `<home>/.mcli`): reading it, which creates nothing and never writes into
//! the process environment, the lookup the human-only commands use, and
//! updating it under a lock.

use std::env::{self, VarError};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::OnceLock;

use super::{ConfigFile, ConfigFileError, Setting};
use crate::identity::configured_dir;
use crate::store::create_private_db_file;
use crate::util::misc::ensure_private_dir;

/// The name of the config file in the data directory.
pub const CONFIG_FILE: &str = "config";

/// The file in the data directory whose lock serializes the updates.
const LOCK_FILE: &str = ".config.lock";

/// Where the config file of the data directory `dir` is.
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

/// The config file of `dir`, read for the networked commands: `None` whenever
/// looking it up fails (it or `dir` does not exist, `dir` is not a directory,
/// the process may not reach it, the path cannot be resolved), so a sandbox
/// that hides the home directory, or a data directory that cannot be used,
/// keeps working with options and variables as before the file existed.
pub fn read_config(dir: &Path) -> Result<Option<ConfigFile>, ConfigFileError> {
    read_with(dir, |_| true)
}

/// The config file of `dir`, read for a command that reports or rewrites it:
/// `None` only when it or `dir` does not exist (or `dir` is not a directory);
/// a file the process may not reach is `Unreadable`.
pub fn read_config_strict(dir: &Path) -> Result<Option<ConfigFile>, ConfigFileError> {
    read_with(dir, |kind| {
        matches!(kind, ErrorKind::NotFound | ErrorKind::NotADirectory)
    })
}

/// Read and parse the config file of `dir`; `is_absent` says which errors of
/// looking it up mean that there is no file. Something that is not a regular
/// file, a failed read and text that is not UTF-8 are `Unreadable`, in both
/// reads.
fn read_with(
    dir: &Path,
    is_absent: fn(ErrorKind) -> bool,
) -> Result<Option<ConfigFile>, ConfigFileError> {
    let path = config_path(dir);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if is_absent(error.kind()) => return Ok(None),
        Err(_) => return Err(ConfigFileError::Unreadable),
    };
    if !metadata.is_file() {
        return Err(ConfigFileError::Unreadable);
    }
    let bytes = fs::read(&path).map_err(|_| ConfigFileError::Unreadable)?;
    let text = String::from_utf8(bytes).map_err(|_| ConfigFileError::Unreadable)?;
    ConfigFile::parse(&text).map(Some)
}

/// A refused config file and where it is. Its text, `<path>: <problem>`,
/// names the line, never the content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigProblem {
    pub path: PathBuf,
    pub error: ConfigFileError,
}

impl fmt::Display for ConfigProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.error)
    }
}

/// The config file of the configured data directory, read for the networked
/// commands ([`read_config`]); `None` without a data directory.
pub fn configured_config() -> Result<Option<ConfigFile>, ConfigProblem> {
    let Some(dir) = configured_dir() else {
        return Ok(None);
    };
    read_config(&dir).map_err(|error| ConfigProblem {
        path: config_path(&dir),
        error,
    })
}

/// The config file the human-only commands read, installed once per run.
static INSTALLED: OnceLock<ConfigFile> = OnceLock::new();

/// Make `file` answer [`var`] for the rest of the run. Only the first call
/// counts; nothing is written into the process environment.
pub fn install(file: ConfigFile) {
    let _ = INSTALLED.set(file);
}

/// `std::env::var` for the human-only commands: a variable that is not
/// present is answered by the installed config file when it is one of the
/// four settings. With nothing installed it is exactly `std::env::var`.
pub fn var(name: &str) -> Result<String, VarError> {
    var_with(INSTALLED.get(), name, env::var(name))
}

/// The environment's answer `env` for `name`, else the value of `file`.
fn var_with(
    file: Option<&ConfigFile>,
    name: &str,
    env: Result<String, VarError>,
) -> Result<String, VarError> {
    match env {
        Err(VarError::NotPresent) => Setting::from_name(name)
            .and_then(|setting| file?.get(setting))
            .map(str::to_owned)
            .ok_or(VarError::NotPresent),
        found => found,
    }
}

/// Why [`update_config`] did not replace the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// A value the setting refuses; nothing was read or written.
    InvalidValue,
    /// The existing file was refused or could not be reached; it was left as
    /// it is.
    Refused(ConfigFileError),
    /// The data directory, the lock or the temporary file could not be
    /// created; nothing was written.
    NotCreated,
    /// The temporary file could not be written or renamed; it was removed
    /// and the old file is intact.
    Failed,
}

/// Set `setting` to `value` (trimmed) in the config file of `dir`, keeping
/// the other settings, and return the file written. The file is read strictly
/// before anything is created, so a refused one is left as it is; then the
/// directory is created one level deep or made owner-only (0700 on Unix) and
/// the file is read again and rewritten while an exclusive lock on
/// `.config.lock` (created owner-only, never removed) is held, so updates made
/// at once by several processes or threads all land.
pub fn update_config(dir: &Path, setting: Setting, value: &str) -> Result<ConfigFile, WriteError> {
    let value = value.trim();
    if !setting.accepts(value) {
        return Err(WriteError::InvalidValue);
    }
    read_config_strict(dir).map_err(WriteError::Refused)?;
    ensure_private_dir(dir).map_err(|_| WriteError::NotCreated)?;
    let _lock = lock(dir).map_err(|_| WriteError::NotCreated)?;
    let file = read_config_strict(dir)
        .map_err(WriteError::Refused)?
        .unwrap_or_default()
        .with(setting, value.to_owned());
    write_config(dir, &file)?;
    Ok(file)
}

/// The lock file of `dir`, locked exclusively until it is dropped.
fn lock(dir: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(dir.join(LOCK_FILE))?;
    file.lock()?;
    Ok(file)
}

/// Replace the config file of the existing directory `dir` with `file`,
/// atomically: a temporary owner-only (0600) file left by an earlier run of
/// this process id is removed, a new one is written, synced and renamed over
/// `config` (which replaces a symlink rather than its target).
fn write_config(dir: &Path, file: &ConfigFile) -> Result<(), WriteError> {
    let temporary = dir.join(format!(".{CONFIG_FILE}.{}.tmp", process::id()));
    match fs::remove_file(&temporary) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(WriteError::NotCreated),
        _ => {}
    }
    create_private_db_file(&temporary).map_err(|_| WriteError::NotCreated)?;
    fill(&temporary, file.render().as_bytes())
        .and_then(|()| fs::rename(&temporary, config_path(dir)))
        .map_err(|_| {
            let _ = fs::remove_file(&temporary);
            WriteError::Failed
        })
}

/// Write `bytes` into the existing file at `path` and sync it.
fn fill(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut out = OpenOptions::new().write(true).open(path)?;
    out.write_all(bytes)?;
    out.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Setting;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A unique directory under the system temp dir, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let seq = SEQ.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("mcli-settings-{}-{seq}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        /// A child directory holding `config` with `bytes`.
        fn with_config(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let dir = self.0.join(name);
            fs::create_dir(&dir).unwrap();
            fs::write(config_path(&dir), bytes).unwrap();
            dir
        }

        /// Every path under the directory, relative and sorted.
        fn tree(&self) -> Vec<PathBuf> {
            fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
                for entry in fs::read_dir(dir).unwrap() {
                    let path = entry.unwrap().path();
                    out.push(path.strip_prefix(root).unwrap().to_path_buf());
                    if path.is_dir() {
                        walk(&path, root, out);
                    }
                }
            }
            let mut out = Vec::new();
            walk(&self.0, &self.0, &mut out);
            out.sort();
            out
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_config_is_none_and_creates_nothing() {
        let root = Dir::new();
        let empty = root.0.join("empty");
        fs::create_dir(&empty).unwrap();
        let regular = root.0.join("regular");
        fs::write(&regular, "a file, not a directory").unwrap();
        let before = root.tree();
        for dir in [root.0.join("absent"), empty, regular] {
            assert_eq!(read_config(&dir), Ok(None), "{dir:?}");
            assert_eq!(read_config_strict(&dir), Ok(None), "{dir:?}");
        }
        assert_eq!(root.tree(), before);

        // A data directory the process may not search: no file for the
        // networked commands, unreadable for a command that reports the file.
        #[cfg(unix)]
        {
            let locked = root.with_config("locked", b"POW=1\n");
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
            // Root reaches the file whatever the mode; the case needs a user it stops.
            if fs::metadata(config_path(&locked)).is_err() {
                assert_eq!(read_config(&locked), Ok(None));
                assert_eq!(
                    read_config_strict(&locked),
                    Err(ConfigFileError::Unreadable)
                );
            }
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    /// A data directory whose path cannot be resolved (a name too long, a
    /// symlink loop) holds no file for the networked commands.
    #[test]
    fn a_data_directory_that_cannot_be_resolved_holds_no_file() {
        let root = Dir::new();
        assert_eq!(read_config(&root.0.join("x".repeat(300))), Ok(None));
        #[cfg(unix)]
        {
            let looped = root.0.join("loop");
            std::os::unix::fs::symlink(&looped, &looped).unwrap();
            assert_eq!(read_config(&looped), Ok(None));
            assert_eq!(
                read_config_strict(&looped),
                Err(ConfigFileError::Unreadable)
            );
        }
    }

    #[test]
    fn a_config_that_cannot_be_read_is_unreadable() {
        let root = Dir::new();
        let as_directory = root.0.join("directory");
        fs::create_dir_all(config_path(&as_directory)).unwrap();
        let not_utf8 = root.with_config("binary", &[b'P', 0xff, 0xfe, b'\n']);
        for dir in [&as_directory, &not_utf8] {
            assert_eq!(
                read_config(dir),
                Err(ConfigFileError::Unreadable),
                "{dir:?}"
            );
            assert_eq!(read_config_strict(dir), Err(ConfigFileError::Unreadable));
        }

        #[cfg(unix)]
        {
            let closed = root.with_config("closed", b"POW=1\n");
            let path = config_path(&closed);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read(&path).is_err() {
                assert_eq!(read_config(&closed), Err(ConfigFileError::Unreadable));
                assert_eq!(
                    read_config_strict(&closed),
                    Err(ConfigFileError::Unreadable)
                );
            }
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        // A refused line and a good file come back as they parse.
        let refused = root.with_config("refused", b"POW=1\nSECRET=x\n");
        assert_eq!(
            read_config(&refused),
            Err(ConfigFileError::KeyNotAllowed { line: 2 })
        );
        let good = root.with_config("good", b"POW=1\n");
        let file = read_config_strict(&good).unwrap().unwrap();
        assert_eq!(file.get(Setting::Pow), Some("1"));
        let problem = ConfigProblem {
            path: config_path(&refused),
            error: ConfigFileError::KeyNotAllowed { line: 2 },
        };
        assert_eq!(
            problem.to_string(),
            format!(
                "{}: line 2 sets a key other than MOSTRO_PUBKEY, RELAYS, POW or TRANSPORT",
                config_path(&refused).display()
            )
        );
    }

    #[test]
    fn the_legacy_var_uses_the_installed_file_only_for_absent_variables() {
        let file = ConfigFile::parse("RELAYS=wss://file\nPOW=3\n").unwrap();
        let absent = || Err(VarError::NotPresent);
        let file_var = |name, env| var_with(Some(&file), name, env);
        assert_eq!(file_var("RELAYS", Ok(String::new())), Ok(String::new()));
        assert_eq!(file_var("RELAYS", absent()), Ok("wss://file".into()));
        assert_eq!(file_var("TRANSPORT", absent()), absent());
        assert_eq!(file_var("SECRET", absent()), absent());
        let not_unicode = Err(VarError::NotUnicode("wss://".into()));
        assert_eq!(file_var("RELAYS", not_unicode.clone()), not_unicode);
        // With nothing installed it is the environment's answer.
        assert_eq!(var_with(None, "RELAYS", absent()), absent());
        for name in ["PATH", "MCLI_TEST_VARIABLE_THAT_IS_NEVER_SET"] {
            assert_eq!(var(name), std::env::var(name), "{name}");
        }
    }

    #[test]
    fn update_creates_an_owner_only_directory_lock_and_file_and_keeps_the_others() {
        let root = Dir::new();
        let dir = root.0.join("data");
        let pow = ConfigFile::parse("POW=3\n").unwrap();
        assert_eq!(update_config(&dir, Setting::Pow, " 3 "), Ok(pow));
        let file = update_config(&dir, Setting::Relays, "wss://a").unwrap();
        assert_eq!(file.render(), "RELAYS=wss://a\nPOW=3\n");
        assert_eq!(read_config_strict(&dir), Ok(Some(file)));
        let data = Path::new("data");
        let tree = [data.to_path_buf(), data.join(LOCK_FILE), config_path(data)];
        assert_eq!(root.tree(), tree);
        #[cfg(unix)]
        assert_eq!(
            [&dir, &config_path(&dir), &dir.join(LOCK_FILE)].map(|path| mode(path)),
            [0o700, 0o600, 0o600]
        );
    }

    /// A value the setting refuses, such as one that would add a line, is
    /// never stored: nothing is read, created or written.
    #[test]
    fn update_refuses_a_value_the_setting_refuses() {
        let root = Dir::new();
        let dir = root.with_config("data", b"POW=1\n");
        let before = root.tree();
        for value in ["wss://a\nMOSTRO_PUBKEY=x", " ", "https://a"] {
            let refused = update_config(&dir, Setting::Relays, value);
            assert_eq!(refused, Err(WriteError::InvalidValue), "{value:?}");
        }
        assert_eq!(fs::read(config_path(&dir)).unwrap(), b"POW=1\n");
        assert_eq!(root.tree(), before);
    }

    /// Updates made at once by several threads (or processes) each keep the
    /// values the others wrote.
    #[test]
    fn concurrent_updates_all_land() {
        let root = Dir::new();
        let key = nostr_sdk::prelude::Keys::generate().public_key().to_hex();
        let values = [
            (Setting::MostroPubkey, key.as_str()),
            (Setting::Relays, "wss://a"),
            (Setting::Pow, "7"),
            (Setting::Transport, "nip44"),
        ];
        for round in 0..10 {
            let dir = root.0.join(format!("round-{round}"));
            std::thread::scope(|scope| {
                for (setting, value) in values {
                    let dir = &dir;
                    scope.spawn(move || update_config(dir, setting, value).unwrap());
                }
            });
            let file = read_config_strict(&dir).unwrap().unwrap();
            for (setting, value) in values {
                assert_eq!(file.get(setting), Some(value), "round {round}");
            }
        }
    }

    /// A data directory whose path is not UTF-8 is written like any other.
    #[cfg(target_os = "linux")]
    #[test]
    fn update_writes_a_data_directory_whose_path_is_not_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let root = Dir::new();
        let dir = root.0.join(std::ffi::OsStr::from_bytes(b"data-\xff"));
        let file = ConfigFile::parse("POW=1\n").unwrap();
        assert_eq!(update_config(&dir, Setting::Pow, "1"), Ok(file));
        assert_eq!(fs::read_to_string(config_path(&dir)).unwrap(), "POW=1\n");
    }

    #[test]
    fn update_replaces_an_existing_file_and_tightens_its_mode() {
        let root = Dir::new();
        let dir = root.with_config("data", b"# old\nexport POW='1'\n");
        let path = config_path(&dir);
        let file = ConfigFile::parse("POW=2\n").unwrap();
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(update_config(&dir, Setting::Pow, "2"), Ok(file.clone()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "POW=2\n");
        #[cfg(unix)]
        {
            assert_eq!(mode(&path), 0o600);
            // A symlinked config becomes a regular file; its target is untouched.
            let target = root.0.join("target");
            fs::write(&target, "POW=9\n").unwrap();
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert_eq!(update_config(&dir, Setting::Pow, "2"), Ok(file));
            assert!(fs::symlink_metadata(&path).unwrap().is_file());
            assert_eq!(fs::read_to_string(&target).unwrap(), "POW=9\n");
        }
    }

    #[test]
    fn a_stale_temporary_of_this_process_is_replaced() {
        let root = Dir::new();
        let dir = root.0.join("data");
        fs::create_dir(&dir).unwrap();
        let stale = dir.join(format!(".config.{}.tmp", std::process::id()));
        fs::write(&stale, "left by a crash").unwrap();
        let file = ConfigFile::parse("POW=1\n").unwrap();
        assert_eq!(write_config(&dir, &file), Ok(()));
        let data = Path::new("data");
        assert_eq!(root.tree(), [data.to_path_buf(), config_path(data)]);
    }

    #[test]
    fn a_write_that_cannot_complete_keeps_the_old_state() {
        let root = Dir::new();
        let file = ConfigFile::parse("POW=1\n").unwrap();
        // `config` is a non-empty directory: the rename fails.
        let blocked = root.0.join("blocked");
        fs::create_dir_all(config_path(&blocked).join("inside")).unwrap();
        // The data directory's path is a regular file.
        let regular = root.0.join("regular");
        fs::write(&regular, "a file, not a directory").unwrap();
        let before = root.tree();
        assert_eq!(write_config(&blocked, &file), Err(WriteError::Failed));
        assert_eq!(write_config(&regular, &file), Err(WriteError::NotCreated));
        // An update reads the file first, so a refused one creates nothing.
        let unreadable = Err(WriteError::Refused(ConfigFileError::Unreadable));
        assert_eq!(update_config(&blocked, Setting::Pow, "1"), unreadable);
        let not_created = Err(WriteError::NotCreated);
        assert_eq!(update_config(&regular, Setting::Pow, "1"), not_created);
        assert_eq!(root.tree(), before);
    }
}
