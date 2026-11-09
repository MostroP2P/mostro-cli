//! Reading the config file `config` in the data directory (`MCLI_DIR`, else
//! `<home>/.mcli`). Reading creates nothing and never writes into the process
//! environment.

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{ConfigFile, ConfigFileError};
use crate::identity::configured_dir;

/// The name of the config file in the data directory.
pub const CONFIG_FILE: &str = "config";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Setting;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};

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
}
