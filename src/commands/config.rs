//! `config set`: the config file of the data directory, read and rewritten
//! under a lock without any node, relay or network. It returns a typed
//! outcome and never prints, prompts or exits.

use std::path::Path;

use crate::outcome::config::{ConfigData, ConfigSetInput, CONFIG_SET_COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Persistence, Warnings};
use crate::settings::{config_path, update_config, ConfigFile, ConfigProblem, WriteError};

pub type SetOutcome = Envelope<ConfigData, ()>;

/// Set one setting in the config file of `dir`, keeping the other settings'
/// values ([`update_config`]). `dir` is `None` when no data directory can be
/// determined. A file that was refused or could not be reached is never
/// touched, and its problem comes back for the caller's hint.
pub fn config_set(
    dir: Option<&Path>,
    input: &ConfigSetInput,
) -> (SetOutcome, Option<ConfigProblem>) {
    let Some(dir) = dir else {
        let outcome = set_failed(
            ErrorCode::ConfigurationUnwritable,
            Persistence::NotAttempted,
        );
        return (outcome, None);
    };
    let result = update_config(dir, input.setting(), input.value());
    let problem = match result {
        Err(WriteError::Refused(error)) => Some(ConfigProblem {
            path: config_path(dir),
            error,
        }),
        _ => None,
    };
    (set_outcome(dir, result), problem)
}

/// The outcome of one update of the config file of `dir`.
pub fn set_outcome(dir: &Path, result: Result<ConfigFile, WriteError>) -> SetOutcome {
    match result {
        Ok(file) => Envelope::ok(
            CONFIG_SET_COMMAND,
            ConfigData::new(Some(&config_path(dir)), Some(&file)),
            Warnings::new(),
            ExecutionMeta::local(Persistence::Applied),
        ),
        Err(WriteError::InvalidValue) => invalid_input(),
        Err(WriteError::Refused(_)) => {
            set_failed(ErrorCode::ConfigurationInvalid, Persistence::NotAttempted)
        }
        Err(WriteError::NotCreated) => set_failed(
            ErrorCode::ConfigurationUnwritable,
            Persistence::NotAttempted,
        ),
        Err(WriteError::Failed) => {
            set_failed(ErrorCode::ConfigurationUnwritable, Persistence::Failed)
        }
    }
}

/// The outcome of a `config set` whose key or value was refused.
pub fn invalid_input() -> SetOutcome {
    set_failed(ErrorCode::InvalidArguments, Persistence::NotApplicable)
}

fn set_failed(code: ErrorCode, persistence: Persistence) -> SetOutcome {
    Envelope::error(
        Some(CONFIG_SET_COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::local(persistence),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ConfigFileError;
    use serde_json::{json, to_value, Value};
    use std::fs;
    use std::path::PathBuf;

    /// The `status`, `error.code` and `meta.persistence` of an outcome.
    fn row(outcome: SetOutcome) -> Value {
        let value = to_value(outcome).unwrap();
        json!([
            value["status"],
            value["error"]["code"],
            value["meta"]["persistence"]
        ])
    }

    fn temporary(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mcli-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn config_set_maps_each_case_to_its_row() {
        let input = ConfigSetInput::new("pow", "4").unwrap();
        let (nowhere, problem) = config_set(None, &input);
        assert_eq!(
            (row(nowhere), problem),
            (
                json!(["error", "configuration_unwritable", "not_attempted"]),
                None
            )
        );
        let root = temporary("set");
        let dir = root.join("data");
        let path = config_path(&dir);
        fs::create_dir(&dir).unwrap();
        fs::write(&path, "SECRET=x\n").unwrap();
        let (refused, problem) = config_set(Some(&dir), &input);
        assert_eq!(
            row(refused),
            json!(["error", "configuration_invalid", "not_attempted"])
        );
        let error = ConfigFileError::KeyNotAllowed { line: 1 };
        let expected = ConfigProblem {
            path: path.clone(),
            error,
        };
        assert_eq!(problem, Some(expected));
        assert_eq!(fs::read(&path).unwrap(), b"SECRET=x\n");

        fs::write(&path, "export RELAYS=\"wss://a , wss://b\"\nPOW=1\n").unwrap();
        let (saved, problem) = config_set(Some(&dir), &input);
        let saved = to_value(saved).unwrap();
        assert_eq!(
            [&saved["status"], &saved["meta"]["persistence"]],
            ["ok", "applied"]
        );
        assert_eq!(problem, None);
        assert_eq!(
            saved["data"],
            json!({"path": path.to_str(), "mostro_pubkey": null, "relays": "wss://a , wss://b", "pow": "4", "transport": null})
        );
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, "RELAYS=wss://a , wss://b\nPOW=4\n");

        let blocked = root.join("blocked");
        fs::create_dir_all(config_path(&blocked).join("inside")).unwrap();
        let (unreachable, problem) = config_set(Some(&blocked), &input);
        assert_eq!(
            row(unreachable),
            json!(["error", "configuration_invalid", "not_attempted"])
        );
        assert_eq!(problem.map(|p| p.error), Some(ConfigFileError::Unreadable));
        for (error, expected) in [
            (
                WriteError::InvalidValue,
                ["invalid_arguments", "not_applicable"],
            ),
            (
                WriteError::NotCreated,
                ["configuration_unwritable", "not_attempted"],
            ),
            (WriteError::Failed, ["configuration_unwritable", "failed"]),
        ] {
            let outcome = set_outcome(&dir, Err(error));
            assert_eq!(row(outcome), json!(["error", expected[0], expected[1]]));
        }
        assert_eq!(
            row(invalid_input()),
            json!(["error", "invalid_arguments", "not_applicable"])
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
