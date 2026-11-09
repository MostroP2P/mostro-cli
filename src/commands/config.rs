//! `config set` and `config show`: the config file of the data directory,
//! rewritten under a lock or only read, without any node, relay or network.
//! They return typed outcomes and never print, prompt or exit.

use std::path::Path;

use crate::outcome::config::{ConfigData, ConfigSetInput, CONFIG_SET_COMMAND, CONFIG_SHOW_COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Persistence, Warnings};
use crate::settings::{
    config_path, read_config_strict, update_config, ConfigFile, ConfigFileError, ConfigProblem,
    WriteError,
};

pub type SetOutcome = Envelope<ConfigData, ()>;
pub type ShowOutcome = Envelope<ConfigData, ()>;

/// Report the config file of `dir` and the values stored in it, read-only
/// and strictly: a file that was refused or could not be reached is an error,
/// never a set of nulls, and its problem comes back for the caller's hint.
/// `dir` is `None` when no data directory can be determined, so there is no
/// file.
pub fn config_show(dir: Option<&Path>) -> (ShowOutcome, Option<ConfigProblem>) {
    let read = dir.map_or(Ok(None), read_config_strict);
    let problem = match (dir, &read) {
        (Some(dir), Err(error)) => Some(ConfigProblem {
            path: config_path(dir),
            error: *error,
        }),
        _ => None,
    };
    (show_outcome(dir, read), problem)
}

/// The outcome of one strict read of the config file of `dir`.
pub fn show_outcome(
    dir: Option<&Path>,
    read: Result<Option<ConfigFile>, ConfigFileError>,
) -> ShowOutcome {
    let meta = ExecutionMeta::read(None);
    match read {
        Ok(file) => {
            let path = dir.map(config_path);
            let data = ConfigData::new(path.as_deref(), file.as_ref());
            Envelope::ok(CONFIG_SHOW_COMMAND, data, Warnings::new(), meta)
        }
        Err(_) => Envelope::error(
            Some(CONFIG_SHOW_COMMAND),
            ErrorCode::ConfigurationInvalid,
            Warnings::new(),
            meta,
        ),
    }
}

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

    #[test]
    fn config_show_reports_the_file_and_refuses_one_it_cannot_read() {
        let data = |outcome: ShowOutcome| to_value(outcome).unwrap()["data"].clone();
        let nulls = json!({"path": null, "mostro_pubkey": null, "relays": null, "pow": null, "transport": null});
        let (nowhere, problem) = config_show(None);
        assert_eq!(row(nowhere.clone()), json!(["ok", null, "not_applicable"]));
        assert_eq!((data(nowhere), problem), (nulls.clone(), None));

        let root = temporary("show");
        let absent = root.join("absent");
        let (empty, problem) = config_show(Some(&absent));
        let path = config_path(&absent);
        let mut expected = nulls;
        expected["path"] = json!(path.to_str());
        assert_eq!((data(empty), problem), (expected, None));
        assert!(!absent.exists());

        let dir = root.join("data");
        fs::create_dir(&dir).unwrap();
        fs::write(config_path(&dir), "export POW='2'\nTRANSPORT=nip44\n").unwrap();
        let (values, problem) = config_show(Some(&dir));
        assert_eq!(problem, None);
        assert_eq!(
            data(values),
            json!({"path": config_path(&dir).to_str(), "mostro_pubkey": null, "relays": null, "pow": "2", "transport": "nip44"})
        );

        fs::write(config_path(&dir), "POW=1\nADMIN_NSEC=x\n").unwrap();
        let (refused, problem) = config_show(Some(&dir));
        assert_eq!(
            row(refused),
            json!(["error", "configuration_invalid", "not_applicable"])
        );
        let error = ConfigFileError::KeyNotAllowed { line: 2 };
        let path = config_path(&dir);
        assert_eq!(problem, Some(ConfigProblem { path, error }));

        fs::remove_file(config_path(&dir)).unwrap();
        fs::create_dir(config_path(&dir)).unwrap();
        let (unreadable, problem) = config_show(Some(&dir));
        assert_eq!(row(unreadable)[1], "configuration_invalid");
        assert_eq!(problem.map(|p| p.error), Some(ConfigFileError::Unreadable));
        fs::remove_dir_all(&root).unwrap();
    }
}
