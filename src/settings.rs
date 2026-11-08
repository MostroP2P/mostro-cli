//! The node settings (`MOSTRO_PUBKEY`, `RELAYS`, `POW`, `TRANSPORT`) and the
//! config file that may hold them (spec 018). This module is pure: the format,
//! the value checks and the precedence. [`file`] reads the file.
//!
//! A setting comes from its root option, else from its environment variable
//! when that is set (even empty), else from the config file. The file holds
//! only these four settings, as `KEY=VALUE` lines with the variable names;
//! any other key is refused, so no secret is ever read from it.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::str::FromStr;

use nostr_sdk::prelude::{PublicKey, RelayUrl};

use crate::msg::{relay_entries, MOSTRO_PUBKEY_VAR, RELAYS_VAR};

pub mod file;

pub use file::{
    config_path, configured_config, install, read_config, read_config_strict, var, ConfigProblem,
};

/// The byte order mark some Windows editors write at the start of a file.
const BOM: char = '\u{feff}';

/// One of the four node settings the config file may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Setting {
    MostroPubkey,
    Relays,
    Pow,
    Transport,
}

impl Setting {
    /// Every setting.
    pub const ALL: [Self; 4] = [Self::MostroPubkey, Self::Relays, Self::Pow, Self::Transport];

    /// Its environment variable, which is also its key in the file.
    pub fn name(self) -> &'static str {
        match self {
            Self::MostroPubkey => MOSTRO_PUBKEY_VAR,
            Self::Relays => RELAYS_VAR,
            Self::Pow => "POW",
            Self::Transport => "TRANSPORT",
        }
    }

    /// The setting whose variable is exactly `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|setting| setting.name() == name)
    }

    /// Whether the commands accept `value` for this setting: a public key (hex
    /// or npub), relay URLs (at least one), a work in bits (0 to 255) or
    /// `nip44`. A value with a control character is never accepted.
    pub fn accepts(self, value: &str) -> bool {
        if value.chars().any(char::is_control) {
            return false;
        }
        match self {
            Self::MostroPubkey => PublicKey::from_str(value).is_ok(),
            Self::Relays => {
                let mut entries = relay_entries(value).peekable();
                entries.peek().is_some() && entries.all(|entry| RelayUrl::parse(entry).is_ok())
            }
            Self::Pow => value.parse::<u8>().is_ok(),
            Self::Transport => value == "nip44",
        }
    }
}

/// The settings a config file holds, each accepted by its check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigFile {
    values: BTreeMap<Setting, String>,
}

impl ConfigFile {
    /// Parse the text of a config file: UTF-8 `KEY=VALUE` lines (LF or CRLF),
    /// one leading byte order mark ignored. Blank lines and lines starting
    /// with `#` are skipped; an `export` prefix is dropped; key and value are
    /// trimmed and one pair of matching `"` or `'` quotes around the value is
    /// removed. There are no inline comments, escapes or expansion. An empty
    /// value means unset and a repeated key keeps its last line. The first
    /// line refused refuses the whole file.
    pub fn parse(text: &str) -> Result<Self, ConfigFileError> {
        let text = text.strip_prefix(BOM).unwrap_or(text);
        let mut values = BTreeMap::new();
        for (index, line) in text.split('\n').enumerate() {
            match entry(line, index + 1)? {
                None => {}
                Some((setting, "")) => {
                    values.remove(&setting);
                }
                Some((setting, value)) => {
                    values.insert(setting, value.to_owned());
                }
            }
        }
        Ok(Self { values })
    }

    /// The value stored for `setting`.
    pub fn get(&self, setting: Setting) -> Option<&str> {
        self.values.get(&setting).map(String::as_str)
    }
}

/// The setting and value of line number `line`; `None` for a blank line or a
/// comment.
fn entry(text: &str, line: usize) -> Result<Option<(Setting, &str)>, ConfigFileError> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('#') {
        return Ok(None);
    }
    let text = match text.strip_prefix("export") {
        Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start(),
        _ => text,
    };
    let (key, value) = text
        .split_once('=')
        .ok_or(ConfigFileError::Malformed { line })?;
    let setting = Setting::from_name(key.trim()).ok_or(ConfigFileError::KeyNotAllowed { line })?;
    let value = unquoted(value.trim());
    if !value.is_empty() && !setting.accepts(value) {
        return Err(ConfigFileError::InvalidValue { line });
    }
    Ok(Some((setting, value)))
}

/// `value` without one pair of matching quotes around all of it.
fn unquoted(value: &str) -> &str {
    ['"', '\'']
        .into_iter()
        .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value)
}

/// Why a config file is refused. The texts name the line, never its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigFileError {
    /// Not a regular file, not readable, or not UTF-8.
    Unreadable,
    /// A line that is neither blank, a comment nor `KEY=VALUE`.
    Malformed { line: usize },
    /// A key other than the four settings.
    KeyNotAllowed { line: usize },
    /// A value its setting refuses.
    InvalidValue { line: usize },
}

impl fmt::Display for ConfigFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable => f.write_str("not a readable UTF-8 text file"),
            Self::Malformed { line } => write!(f, "line {line} is not KEY=VALUE"),
            Self::KeyNotAllowed { line } => write!(
                f,
                "line {line} sets a key other than MOSTRO_PUBKEY, RELAYS, POW or TRANSPORT"
            ),
            Self::InvalidValue { line } => write!(f, "line {line} has a value its setting refuses"),
        }
    }
}

impl std::error::Error for ConfigFileError {}

/// The value of the setting named `name`: the environment's when it is set,
/// even empty, else the file's. A name outside the four settings is answered
/// by the environment only. Root options come before this and are the
/// caller's.
pub fn setting(
    file: Option<&ConfigFile>,
    name: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    env(name).or_else(|| {
        let setting = Setting::from_name(name)?;
        file?.get(setting).map(str::to_owned)
    })
}

/// The text of an environment variable for [`setting`]: `None` when it is
/// unset. A set value that is not valid Unicode is empty, so it still wins
/// over the file and, as before the file existed, counts as blank.
pub fn env_text(value: Option<OsString>) -> Option<String> {
    value.map(|value| value.into_string().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::{Keys, ToBech32};

    const SENTINEL: &str = "secret-sentinel-7f3a";

    fn hex() -> String {
        Keys::generate().public_key().to_hex()
    }

    fn parsed(text: &str) -> ConfigFile {
        ConfigFile::parse(text).unwrap_or_else(|error| panic!("{text:?}: {error}"))
    }

    #[test]
    fn parse_reads_the_four_keys_comments_export_and_quotes() {
        let key = hex();
        let text = format!(
            "\u{feff}# node\n\n   # indented comment\nexport MOSTRO_PUBKEY=\"{key}\"\r\n\
             export\tRELAYS = 'wss://a.example , wss://b.example'\n  POW=3  \nTRANSPORT=nip44\n"
        );
        let file = parsed(&text);
        assert_eq!(file.get(Setting::MostroPubkey), Some(key.as_str()));
        assert_eq!(
            file.get(Setting::Relays),
            Some("wss://a.example , wss://b.example")
        );
        assert_eq!(file.get(Setting::Pow), Some("3"));
        assert_eq!(file.get(Setting::Transport), Some("nip44"));

        // An empty value is unset, and a repeated key keeps its last line.
        let file = parsed("POW=3\nPOW=\nTRANSPORT=\"\"\nRELAYS=wss://a\nRELAYS=wss://b\n");
        assert_eq!(file.get(Setting::Pow), None);
        assert_eq!(file.get(Setting::Transport), None);
        assert_eq!(file.get(Setting::Relays), Some("wss://b"));
        assert_eq!(parsed(""), ConfigFile::default());

        // The README env.sh block, with real values in its placeholders.
        let npub = Keys::generate().public_key().to_bech32().unwrap();
        let readme = format!(
            "# ~/.config/mostro/env.sh   (chmod 600)\n\
             export MOSTRO_PUBKEY=\"{npub}\"\n\
             export RELAYS=\"wss://relay.example,wss://other.example\"\n\
             # export POW=24                # only for more than 20 bits, or the legacy commands\n\
             # export ADMIN_NSEC=nsec1...   # only if you're an admin/solver\n"
        );
        let file = parsed(&readme);
        assert_eq!(file.get(Setting::MostroPubkey), Some(npub.as_str()));
        assert_eq!(
            file.get(Setting::Relays),
            Some("wss://relay.example,wss://other.example")
        );
        assert_eq!(
            (file.get(Setting::Pow), file.get(Setting::Transport)),
            (None, None)
        );
    }

    #[test]
    fn other_keys_and_lines_without_equals_are_refused_by_line_number() {
        for key in [
            "ADMIN_NSEC",
            "MOSTRO_RPC_TOKEN",
            "MCLI_DIR",
            "SECRET",
            "relays",
            "MOSTROPUBKEY",
        ] {
            let text = format!("# first\nPOW=1\n{key}={SENTINEL}\n");
            let error = ConfigFile::parse(&text).unwrap_err();
            assert_eq!(error, ConfigFileError::KeyNotAllowed { line: 3 }, "{key}");
            let shown = error.to_string();
            assert!(
                shown.contains("line 3") && !shown.contains(SENTINEL),
                "{shown}"
            );
        }
        let error = ConfigFile::parse("POW=1\nRELAYS\n").unwrap_err();
        assert_eq!(error, ConfigFileError::Malformed { line: 2 });
        assert_eq!(error.to_string(), "line 2 is not KEY=VALUE");
        // The first refused line decides.
        assert_eq!(
            ConfigFile::parse("SECRET=x\nRELAYS\n").unwrap_err(),
            ConfigFileError::KeyNotAllowed { line: 1 }
        );
    }

    #[test]
    fn values_the_commands_refuse_are_refused_by_line_number() {
        let nsec = Keys::generate().secret_key().to_bech32().unwrap();
        let refused = [
            ("MOSTRO_PUBKEY", format!("{nsec}{SENTINEL}")),
            ("MOSTRO_PUBKEY", "not-a-key".to_string()),
            ("RELAYS", "https://relay.example".to_string()),
            ("RELAYS", " , ".to_string()),
            ("POW", "256".to_string()),
            ("POW", "-1".to_string()),
            ("POW", "x".to_string()),
            ("TRANSPORT", "gift-wrap".to_string()),
            ("RELAYS", "wss://a\u{0}b".to_string()),
            ("RELAYS", "wss://a\u{1b}b".to_string()),
            ("RELAYS", "wss://a\rb".to_string()),
        ];
        for (key, value) in refused {
            let error = ConfigFile::parse(&format!("\n{key}={value}\n")).unwrap_err();
            assert_eq!(error, ConfigFileError::InvalidValue { line: 2 }, "{key}");
            let shown = error.to_string();
            assert!(
                !shown.contains(&value) && !shown.contains(SENTINEL),
                "{shown}"
            );
        }
        let npub = Keys::generate().public_key().to_bech32().unwrap();
        for (key, value) in [
            ("MOSTRO_PUBKEY", hex()),
            ("MOSTRO_PUBKEY", npub),
            ("RELAYS", "wss://a , wss://b".to_string()),
            ("POW", "0".to_string()),
            ("POW", "20".to_string()),
            ("TRANSPORT", "nip44".to_string()),
        ] {
            let setting = Setting::from_name(key).unwrap();
            assert!(setting.accepts(&value), "{key}={value}");
            assert_eq!(
                parsed(&format!("{key}={value}")).get(setting),
                Some(value.as_str())
            );
        }
    }

    #[test]
    fn a_set_variable_wins_over_the_file_even_when_empty() {
        let file = parsed("RELAYS=wss://file\nPOW=3\n");
        let unset = |_: &str| None;
        let empty = |_: &str| Some(String::new());
        assert_eq!(setting(Some(&file), "RELAYS", empty), Some(String::new()));
        assert_eq!(
            setting(Some(&file), "RELAYS", unset),
            Some("wss://file".to_string())
        );
        assert_eq!(setting(None, "RELAYS", unset), None);
        assert_eq!(setting(Some(&file), "TRANSPORT", unset), None);
        // A name outside the four is answered by the environment only.
        assert_eq!(setting(Some(&file), "ADMIN_NSEC", unset), None);
        assert_eq!(
            setting(Some(&file), "MCLI_DIR", |_| Some("x".to_string())),
            Some("x".to_string())
        );
    }

    #[test]
    fn a_variable_that_is_not_unicode_is_set_but_empty() {
        assert_eq!(env_text(None), None);
        assert_eq!(env_text(Some("wss://e".into())), Some("wss://e".into()));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let bytes = || env_text(Some(OsString::from_vec(b"wss://\xff".to_vec())));
            assert_eq!(bytes(), Some(String::new()));
            // So it still wins over the file, as an empty variable does.
            let file = parsed("RELAYS=wss://file\n");
            assert_eq!(setting(Some(&file), "RELAYS", |_| bytes()), Some("".into()));
        }
    }
}
