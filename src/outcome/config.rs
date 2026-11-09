//! `config.set` and `config.show` data: the config file's path and the four
//! node settings stored in it, as stored. The values are only ones the
//! commands accept (a key, relay URLs, a number, `nip44`); this is not a
//! secrecy promise, since a hex private key can pass the public key check.

use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use super::schema::OutcomeRules;
use super::ErrorCode;
use crate::settings::{ConfigFile, Setting};

/// Canonical name of `config set`.
pub const CONFIG_SET_COMMAND: &str = "config.set";

/// Codes that fail `config.set`.
pub const CONFIG_SET_ERRORS: [ErrorCode; 4] = [
    ErrorCode::InvalidArguments,
    ErrorCode::InternalError,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::ConfigurationUnwritable,
];

/// What `config.set` outcomes may contain: a local write.
pub fn config_set_rules() -> OutcomeRules<'static> {
    OutcomeRules::local(CONFIG_SET_COMMAND, &CONFIG_SET_ERRORS)
}

/// Canonical name of `config show`.
pub const CONFIG_SHOW_COMMAND: &str = "config.show";

/// Codes that fail `config.show`.
pub const CONFIG_SHOW_ERRORS: [ErrorCode; 3] = [
    ErrorCode::InvalidArguments,
    ErrorCode::InternalError,
    ErrorCode::ConfigurationInvalid,
];

/// What `config.show` outcomes may contain: a local read, with no snapshot.
pub fn config_show_rules() -> OutcomeRules<'static> {
    OutcomeRules::read(CONFIG_SHOW_COMMAND, &CONFIG_SHOW_ERRORS, false)
}

/// A checked `config set` request: a setting and its trimmed value, which
/// the setting accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigSetInput {
    setting: Setting,
    value: String,
}

impl ConfigSetInput {
    /// `None` when `key` is not a `config set` key or the setting refuses the
    /// trimmed `value` (an empty one included).
    pub fn new(key: &str, value: &str) -> Option<Self> {
        let setting = Setting::from_key(key)?;
        let value = value.trim();
        (!value.is_empty() && setting.accepts(value)).then(|| Self {
            setting,
            value: value.to_owned(),
        })
    }

    pub fn setting(&self) -> Setting {
        self.setting
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// The config file and the settings stored in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConfigData {
    /// The config file's path; null when no data directory can be determined
    /// or its path is not UTF-8.
    path: Option<String>,
    /// `MOSTRO_PUBKEY` as stored in the file; null when absent.
    mostro_pubkey: Option<String>,
    /// `RELAYS` as stored in the file; null when absent.
    relays: Option<String>,
    /// `POW` as stored in the file; null when absent.
    pow: Option<String>,
    /// `TRANSPORT` as stored in the file; null when absent.
    transport: Option<String>,
}

impl ConfigData {
    /// The file at `path` holding `file`; `None` for a file that does not
    /// exist (every value null).
    pub fn new(path: Option<&Path>, file: Option<&ConfigFile>) -> Self {
        let value = |setting| file.and_then(|file| file.get(setting)).map(str::to_owned);
        Self {
            path: path.and_then(Path::to_str).map(str::to_owned),
            mostro_pubkey: value(Setting::MostroPubkey),
            relays: value(Setting::Relays),
            pow: value(Setting::Pow),
            transport: value(Setting::Transport),
        }
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// The value stored for `setting`.
    pub fn value(&self, setting: Setting) -> Option<&str> {
        match setting {
            Setting::MostroPubkey => self.mostro_pubkey.as_deref(),
            Setting::Relays => self.relays.as_deref(),
            Setting::Pow => self.pow.as_deref(),
            Setting::Transport => self.transport.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::schema::output_schema;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::{Envelope, ExecutionMeta, Persistence, Warnings};
    use nostr_sdk::prelude::{Keys, ToBech32};
    use serde_json::{json, to_value, Value};

    const PATH: &str = "/home/user/.mcli/config";

    fn data() -> ConfigData {
        let file = ConfigFile::parse("RELAYS=wss://a\nPOW=3\n").unwrap();
        ConfigData::new(Some(Path::new(PATH)), Some(&file))
    }

    fn ok(command: &'static str, data: ConfigData, persistence: Persistence) -> Value {
        let meta = ExecutionMeta::local(persistence);
        to_value(Envelope::<_, ()>::ok(command, data, Warnings::new(), meta)).unwrap()
    }

    fn error(command: &'static str, code: ErrorCode, persistence: Persistence) -> Value {
        let meta = ExecutionMeta::local(persistence);
        to_value(Envelope::<ConfigData, ()>::error(
            Some(command),
            code,
            Warnings::new(),
            meta,
        ))
        .unwrap()
    }

    /// Whether every outcome conforms to `schema` and every edited copy does not.
    fn check(schema: &Value, good: &[Value], bad: &[(&str, Value)]) {
        for outcome in good {
            assert!(conforms(schema, schema, outcome), "{outcome}");
        }
        for (label, outcome) in bad {
            assert!(!conforms(schema, schema, outcome), "{label}: {outcome}");
        }
    }

    fn edited(outcome: &Value, edit: impl Fn(&mut Value)) -> Value {
        let mut outcome = outcome.clone();
        edit(&mut outcome);
        outcome
    }

    #[test]
    fn set_input_takes_the_four_keys_and_values_the_commands_accept() {
        let input = ConfigSetInput::new("relays", "  wss://a , wss://b \n").unwrap();
        assert_eq!(
            (input.setting(), input.value()),
            (Setting::Relays, "wss://a , wss://b")
        );
        let key = Keys::generate().public_key();
        for (name, value) in [
            ("mostro-pubkey", key.to_hex()),
            ("mostro-pubkey", key.to_bech32().unwrap()),
            ("pow", "0".into()),
            ("pow", "255".into()),
            ("transport", "nip44".into()),
        ] {
            assert!(
                ConfigSetInput::new(name, &value).is_some(),
                "{name}={value}"
            );
        }
        let nsec = Keys::generate().secret_key().to_bech32().unwrap();
        for (name, value) in [
            ("admin-nsec", "x".to_string()),
            ("MOSTRO_PUBKEY", key.to_hex()),
            ("mostro-pubkey", nsec),
            ("relays", "https://relay.example".into()),
            ("pow", "256".into()),
            ("transport", "gift-wrap".into()),
            ("relays", "   ".into()),
            ("relays", "wss://a\nwss://b".into()),
        ] {
            assert!(ConfigSetInput::new(name, &value).is_none(), "{name}");
        }
    }

    #[test]
    fn the_data_holds_the_path_and_the_stored_values() {
        assert_eq!(
            to_value(data()).unwrap(),
            json!({"path": PATH, "mostro_pubkey": null, "relays": "wss://a", "pow": "3", "transport": null})
        );
        let nowhere = ConfigData::new(None, None);
        assert_eq!(nowhere.path(), None);
        assert!(Setting::ALL.iter().all(|s| nowhere.value(*s).is_none()));
    }

    #[test]
    fn every_outcome_of_config_set_conforms_and_others_do_not() {
        let schema = Value::Object(output_schema::<ConfigData, ()>(&config_set_rules()));
        let saved = ok(CONFIG_SET_COMMAND, data(), Persistence::Applied);
        let invalid = error(
            CONFIG_SET_COMMAND,
            ErrorCode::ConfigurationInvalid,
            Persistence::NotAttempted,
        );
        let mut good = vec![saved.clone(), invalid.clone()];
        for (code, persistence) in [
            (ErrorCode::InvalidArguments, Persistence::NotApplicable),
            (
                ErrorCode::ConfigurationUnwritable,
                Persistence::NotAttempted,
            ),
            (ErrorCode::ConfigurationUnwritable, Persistence::Failed),
            (ErrorCode::InternalError, Persistence::NotApplicable),
        ] {
            good.push(error(CONFIG_SET_COMMAND, code, persistence));
        }
        let bad = [
            (
                "published",
                edited(&saved, |o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "data on an error",
                edited(&invalid, |o| o["data"] = saved["data"].clone()),
            ),
            (
                "applied on an error",
                edited(&invalid, |o| o["meta"]["persistence"] = json!("applied")),
            ),
            ("a number", edited(&saved, |o| o["data"]["pow"] = json!(3))),
            (
                "a code of show",
                edited(&invalid, |o| o["error"]["code"] = json!("identity_missing")),
            ),
        ];
        check(&schema, &good, &bad);
    }

    #[test]
    fn every_outcome_of_config_show_conforms_and_others_do_not() {
        let schema = Value::Object(output_schema::<ConfigData, ()>(&config_show_rules()));
        let meta = || ExecutionMeta::read(None);
        let shown = |data| {
            to_value(Envelope::<_, ()>::ok(
                CONFIG_SHOW_COMMAND,
                data,
                Warnings::new(),
                meta(),
            ))
            .unwrap()
        };
        let failed = |code| {
            let outcome = Envelope::<ConfigData, ()>::error(
                Some(CONFIG_SHOW_COMMAND),
                code,
                Warnings::new(),
                meta(),
            );
            to_value(outcome).unwrap()
        };
        let values = shown(data());
        let invalid = failed(ErrorCode::ConfigurationInvalid);
        let mut good = vec![values.clone(), shown(ConfigData::new(None, None))];
        good.extend(CONFIG_SHOW_ERRORS.map(failed));
        let bad = [
            (
                "applied",
                edited(&values, |o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "a code of set",
                edited(&invalid, |o| {
                    o["error"]["code"] = json!("configuration_unwritable")
                }),
            ),
            (
                "data on an error",
                edited(&invalid, |o| o["data"] = values["data"].clone()),
            ),
        ];
        check(&schema, &good, &bad);
    }
}
