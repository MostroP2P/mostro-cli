use mostro_client::util::misc::{get_mcli_path, uppercase_first};

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

#[test]
fn test_get_mcli_path_contains_home() {
    let path = get_mcli_path();
    let home_dir = dirs::home_dir().expect("Couldn't get home directory");
    let home_str = home_dir.to_string_lossy();

    // Path should start with home directory
    assert!(path.starts_with(home_str.as_ref()));
}
