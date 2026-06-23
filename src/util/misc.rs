use std::io;

pub fn uppercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn get_mcli_path() -> String {
    let home_dir = dirs::home_dir().expect("Couldn't get home directory");
    let mcli_path = format!("{}/.mcli", home_dir.display());
    if let Err(e) = ensure_private_dir(&mcli_path) {
        panic!("Couldn't create mostro-cli directory in HOME: {}", e);
    }
    mcli_path
}

/// Ensure `path` exists as a directory that is private to the current user.
///
/// The `.mcli` directory holds `mcli.db`, which in turn stores the mnemonic that
/// derives the user's Mostro identity and trade keys. To avoid that secret being
/// exposed to other local users under a permissive `umask`, the directory is
/// created with mode `0700` on Unix. We also tighten the permissions of a
/// pre-existing directory, so users who created `~/.mcli` with an older version
/// (or a loose `umask`) get hardened on the next run. See issue #179.
pub fn ensure_private_dir(path: &str) -> io::Result<()> {
    match create_dir_private(path) {
        Ok(()) => {}
        // Another thread/process won the race, or the path already existed.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    // `AlreadyExists` only tells us *something* is there. If it's a regular file
    // (or anything other than a directory) we must not chmod it and claim
    // success — callers were promised a private directory.
    if !std::fs::metadata(path)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} exists but is not a directory", path),
        ));
    }

    #[cfg(unix)]
    set_mode(path, 0o700)?;

    Ok(())
}

/// Create a directory restricted to the owner (mode `0700` on Unix).
///
/// Returns an `AlreadyExists` error when the directory is already present, so
/// callers can treat that case as success.
#[cfg(unix)]
fn create_dir_private(path: &str) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_dir_private(path: &str) -> io::Result<()> {
    std::fs::create_dir(path)
}

/// Set the Unix permission bits of `path` to `mode`.
#[cfg(unix)]
pub fn set_mode(path: &str, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}
