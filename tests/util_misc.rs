use mostro_client::util::misc::{ensure_private_dir, get_mcli_path, uppercase_first};

#[test]
fn test_uppercase_first_empty_string() {
    let result = uppercase_first("");
    assert_eq!(result, "");
}

#[test]
fn test_uppercase_first_single_char() {
    let result = uppercase_first("a");
    assert_eq!(result, "A");
}

#[test]
fn test_uppercase_first_already_uppercase() {
    let result = uppercase_first("Hello");
    assert_eq!(result, "Hello");
}

#[test]
fn test_uppercase_first_lowercase_word() {
    let result = uppercase_first("hello");
    assert_eq!(result, "Hello");
}

#[test]
fn test_uppercase_first_multiple_words() {
    let result = uppercase_first("hello world");
    assert_eq!(result, "Hello world");
}

#[test]
fn test_uppercase_first_special_chars() {
    let result = uppercase_first("!hello");
    assert_eq!(result, "!hello");
}

#[test]
fn test_uppercase_first_unicode() {
    let result = uppercase_first("über");
    assert_eq!(result, "Über");
}

#[test]
fn test_uppercase_first_numeric() {
    let result = uppercase_first("123abc");
    assert_eq!(result, "123abc");
}

#[test]
fn test_uppercase_first_whitespace() {
    let result = uppercase_first(" hello");
    assert_eq!(result, " hello");
}

#[test]
fn test_get_mcli_path_returns_valid_path() {
    let path = get_mcli_path();

    // Should return a non-empty string
    assert!(!path.is_empty());

    // Should contain the mcli directory name
    assert!(path.contains(".mcli"));
}

#[test]
fn test_get_mcli_path_is_absolute() {
    let path = get_mcli_path();

    // On Unix systems, should start with /
    // On Windows, should contain :\
    #[cfg(unix)]
    assert!(path.starts_with('/'));

    #[cfg(windows)]
    assert!(path.contains(":\\"));
}

#[test]
fn test_get_mcli_path_consistent() {
    let path1 = get_mcli_path();
    let path2 = get_mcli_path();

    // Should return the same path on multiple calls
    assert_eq!(path1, path2);
}

/// Build a unique, non-existent path inside the OS temp dir without pulling in
/// extra crates or the forbidden `Date`/random APIs.
fn unique_temp_path(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("mcli-test-{}-{}-{}", label, std::process::id(), n))
}

#[cfg(unix)]
#[test]
fn test_ensure_private_dir_creates_with_0700() {
    use std::os::unix::fs::PermissionsExt;

    let dir = unique_temp_path("dir");
    let dir_str = dir.to_str().unwrap();

    ensure_private_dir(dir_str).expect("should create directory");

    assert!(dir.is_dir(), "directory should exist");
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "directory must be private to the owner");

    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn test_ensure_private_dir_tightens_existing_dir() {
    use std::os::unix::fs::PermissionsExt;

    let dir = unique_temp_path("loose");
    let dir_str = dir.to_str().unwrap();

    // Simulate a directory left world-readable by a permissive umask.
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    ensure_private_dir(dir_str).expect("should tighten existing directory");

    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "existing directory must be tightened to 0700");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_ensure_private_dir_is_idempotent() {
    let dir = unique_temp_path("idem");
    let dir_str = dir.to_str().unwrap();

    ensure_private_dir(dir_str).expect("first call should succeed");
    ensure_private_dir(dir_str).expect("second call should succeed");

    assert!(dir.is_dir());

    std::fs::remove_dir_all(&dir).ok();
}
