//! Machine-readable outcome envelope, contract version 1.
//!
//! An [`Envelope`] can only be built through its constructors, so status,
//! data and error always agree. Every key is serialized, as null when empty.
//! Error and warning messages are fixed prose chosen by their code; no
//! caller-provided text reaches the wire.

use serde::Serialize;

pub mod order;

/// Contract version, independent of the executable version.
pub const SCHEMA_VERSION: &str = "1";
/// Exit status for argument and unsupported-output errors.
pub const ARGUMENT_ERROR_EXIT: i32 = 2;
/// Exit status for configuration, read and internal errors.
pub const RUNTIME_ERROR_EXIT: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Error,
    /// Reserved for future write commands; no pilot constructor emits it.
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArguments,
    OutputNotSupported,
    ConfigurationMissing,
    ConfigurationInvalid,
    RelayConnectionFailed,
    RelayReadFailed,
    ReadTimeout,
    ReadLimitExceeded,
    InternalError,
}

impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::InvalidArguments | Self::OutputNotSupported => ARGUMENT_ERROR_EXIT,
            Self::ConfigurationMissing
            | Self::ConfigurationInvalid
            | Self::RelayConnectionFailed
            | Self::RelayReadFailed
            | Self::ReadTimeout
            | Self::ReadLimitExceeded
            | Self::InternalError => RUNTIME_ERROR_EXIT,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidArguments => "The command arguments are invalid.",
            Self::OutputNotSupported => "This command does not support JSON output.",
            Self::ConfigurationMissing => "Required node configuration is missing.",
            Self::ConfigurationInvalid => "The node configuration is invalid.",
            Self::RelayConnectionFailed => "A configured relay could not be connected.",
            Self::RelayReadFailed => "A configured relay failed during the public order read.",
            Self::ReadTimeout => "The public order read did not complete before its deadline.",
            Self::ReadLimitExceeded => "The public order read exceeded its event limit.",
            Self::InternalError => "An internal error occurred.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ErrorInfo {
    code: ErrorCode,
    message: &'static str,
}

impl ErrorInfo {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    InvalidEventTimestamp,
    CleanupIncomplete,
}

impl WarningCode {
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidEventTimestamp => {
                "One or more event timestamps cannot be shown as a UTC date."
            }
            Self::CleanupIncomplete => "Relay cleanup did not finish within its time budget.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Warning {
    code: WarningCode,
    message: &'static str,
}

/// Warnings of one outcome, at most one per code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Warnings(Vec<Warning>);

impl Warnings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `code`; repeated occurrences aggregate into one warning.
    pub fn add(&mut self, code: WarningCode) {
        if !self.contains(code) {
            self.0.push(Warning {
                code,
                message: code.message(),
            });
        }
    }

    pub fn contains(&self, code: WarningCode) -> bool {
        self.0.iter().any(|warning| warning.code == code)
    }

    /// Record every warning of `other`, still one per code.
    pub fn merge(&mut self, other: &Warnings) {
        for warning in &other.0 {
            self.add(warning.code);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Publication {
    NotAttempted,
    Unknown,
    Published,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonAcceptance {
    Unknown,
    Accepted,
    Rejected,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    NotAttempted,
    Applied,
    Failed,
    NotApplicable,
}

/// Execution stages and optional snapshot evidence of one outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExecutionMeta<S> {
    publication: Publication,
    daemon_acceptance: DaemonAcceptance,
    persistence: Persistence,
    snapshot: Option<S>,
}

impl<S> ExecutionMeta<S> {
    /// A known read-only command: no write stage applies. `snapshot` is
    /// `None` until the public query has started.
    pub fn read(snapshot: Option<S>) -> Self {
        Self {
            publication: Publication::NotApplicable,
            daemon_acceptance: DaemonAcceptance::NotApplicable,
            persistence: Persistence::NotApplicable,
            snapshot,
        }
    }

    /// A legacy command rejected before execution, or an invocation whose
    /// command could not be resolved.
    pub fn not_executed() -> Self {
        Self {
            publication: Publication::NotAttempted,
            daemon_acceptance: DaemonAcceptance::Unknown,
            persistence: Persistence::NotAttempted,
            snapshot: None,
        }
    }
}

/// One controlled outcome. `D` is the success data and `S` the snapshot type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Envelope<D, S> {
    schema_version: &'static str,
    command: Option<&'static str>,
    status: Status,
    data: Option<D>,
    error: Option<ErrorInfo>,
    warnings: Warnings,
    meta: ExecutionMeta<S>,
}

impl<D, S> Envelope<D, S> {
    /// A success of the resolved canonical `command`.
    pub fn ok(command: &'static str, data: D, warnings: Warnings, meta: ExecutionMeta<S>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: Some(command),
            status: Status::Ok,
            data: Some(data),
            error: None,
            warnings,
            meta,
        }
    }

    /// A failure; `command` is `None` when it could not be resolved. Data is
    /// always null, even when some results were collected.
    pub fn error(
        command: Option<&'static str>,
        code: ErrorCode,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command,
            status: Status::Error,
            data: None,
            error: Some(ErrorInfo::new(code)),
            warnings,
            meta,
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn command(&self) -> Option<&'static str> {
        self.command
    }

    pub fn data(&self) -> Option<&D> {
        self.data.as_ref()
    }

    pub fn error_code(&self) -> Option<ErrorCode> {
        self.error.map(|error| error.code)
    }

    /// An `internal_error` outcome for the same command, stages and warnings,
    /// without data or snapshot, for when this outcome cannot be serialized.
    pub fn as_internal_error(&self) -> Envelope<(), ()> {
        Envelope::error(
            self.command,
            ErrorCode::InternalError,
            self.warnings.clone(),
            ExecutionMeta {
                publication: self.meta.publication,
                daemon_acceptance: self.meta.daemon_acceptance,
                persistence: self.meta.persistence,
                snapshot: None,
            },
        )
    }

    pub fn exit_code(&self) -> i32 {
        self.error.map_or(0, |error| error.code.exit_code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const EXAMPLES: &str = include_str!("../specs/001-order-list-json/contracts/examples.json");
    const ENVELOPE_KEYS: [&str; 7] = [
        "command",
        "data",
        "error",
        "meta",
        "schema_version",
        "status",
        "warnings",
    ];
    const META_KEYS: [&str; 4] = [
        "daemon_acceptance",
        "persistence",
        "publication",
        "snapshot",
    ];
    const ERROR_CODES: [ErrorCode; 9] = [
        ErrorCode::InvalidArguments,
        ErrorCode::OutputNotSupported,
        ErrorCode::ConfigurationMissing,
        ErrorCode::ConfigurationInvalid,
        ErrorCode::RelayConnectionFailed,
        ErrorCode::RelayReadFailed,
        ErrorCode::ReadTimeout,
        ErrorCode::ReadLimitExceeded,
        ErrorCode::InternalError,
    ];
    const WARNING_CODES: [WarningCode; 2] = [
        WarningCode::InvalidEventTimestamp,
        WarningCode::CleanupIncomplete,
    ];

    type Json = Envelope<Value, Value>;

    fn example(name: &str) -> (i32, Value) {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        let case = examples["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("no example named {name}"));
        let exit = i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap();
        (exit, case["stdout"].clone())
    }

    fn wire<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap()
    }

    fn keys(value: &Value) -> Vec<&str> {
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        keys
    }

    /// Contract name and exit status; exhaustive, so a new code fails to
    /// compile until the contract table is updated here.
    fn error_contract(code: ErrorCode) -> (&'static str, i32) {
        match code {
            ErrorCode::InvalidArguments => ("invalid_arguments", 2),
            ErrorCode::OutputNotSupported => ("output_not_supported", 2),
            ErrorCode::ConfigurationMissing => ("configuration_missing", 1),
            ErrorCode::ConfigurationInvalid => ("configuration_invalid", 1),
            ErrorCode::RelayConnectionFailed => ("relay_connection_failed", 1),
            ErrorCode::RelayReadFailed => ("relay_read_failed", 1),
            ErrorCode::ReadTimeout => ("read_timeout", 1),
            ErrorCode::ReadLimitExceeded => ("read_limit_exceeded", 1),
            ErrorCode::InternalError => ("internal_error", 1),
        }
    }

    fn warning_contract(code: WarningCode) -> &'static str {
        match code {
            WarningCode::InvalidEventTimestamp => "invalid_event_timestamp",
            WarningCode::CleanupIncomplete => "cleanup_incomplete",
        }
    }

    fn assert_safe_message(message: &str) {
        assert!(!message.is_empty());
        assert!(message.is_ascii(), "{message:?}");
        assert!(!message.chars().any(|c| c.is_control()), "{message:?}");
        assert!(
            message.starts_with(|c: char| c.is_ascii_uppercase()),
            "{message:?}"
        );
        assert!(message.ends_with('.'), "{message:?}");
        for marker in ["://", "{", "}", "nsec", "Error:"] {
            assert!(!message.contains(marker), "{message:?} contains {marker:?}");
        }
    }

    #[test]
    fn success_examples_are_reproduced() {
        for name in ["large_amount_success", "empty_success"] {
            let (exit, expected) = example(name);
            let snapshot = expected["meta"]["snapshot"].clone();
            let envelope = Json::ok(
                "order.list",
                expected["data"].clone(),
                Warnings::new(),
                ExecutionMeta::read(Some(snapshot)),
            );
            assert_eq!(wire(&envelope), expected, "{name}");
            assert_eq!(envelope.status(), Status::Ok);
            assert_eq!(envelope.exit_code(), exit, "{name}");
        }
    }

    #[test]
    fn error_examples_are_reproduced() {
        let (exit, expected) = example("partial_relay_timeout");
        let snapshot = expected["meta"]["snapshot"].clone();
        let timeout = Json::error(
            Some("order.list"),
            ErrorCode::ReadTimeout,
            Warnings::new(),
            ExecutionMeta::read(Some(snapshot)),
        );
        assert_eq!((wire(&timeout), timeout.exit_code()), (expected, exit));

        let (exit, expected) = example("invalid_arguments");
        let invalid = Json::error(
            Some("order.list"),
            ErrorCode::InvalidArguments,
            Warnings::new(),
            ExecutionMeta::read(None),
        );
        assert_eq!((wire(&invalid), invalid.exit_code()), (expected, exit));

        let (exit, expected) = example("unsupported_secret_command");
        let unsupported = Json::error(
            Some("account.trade-key"),
            ErrorCode::OutputNotSupported,
            Warnings::new(),
            ExecutionMeta::not_executed(),
        );
        assert_eq!(
            (wire(&unsupported), unsupported.exit_code()),
            (expected, exit)
        );
    }

    #[test]
    fn every_key_is_present_and_null_when_empty() {
        let success = wire(&Json::ok(
            "schema",
            json!({}),
            Warnings::new(),
            ExecutionMeta::read(None),
        ));
        let unresolved = wire(&Json::error(
            None,
            ErrorCode::InvalidArguments,
            Warnings::new(),
            ExecutionMeta::not_executed(),
        ));
        for outcome in [&success, &unresolved] {
            assert_eq!(keys(outcome), ENVELOPE_KEYS);
            assert_eq!(keys(&outcome["meta"]), META_KEYS);
            assert_eq!(outcome["schema_version"], json!(SCHEMA_VERSION));
            assert_eq!(outcome["warnings"], json!([]));
            assert_eq!(outcome["meta"]["snapshot"], Value::Null);
        }
        assert_eq!(success["schema_version"], json!("1"));
        assert_eq!(success["error"], Value::Null);
        assert_eq!(unresolved["command"], Value::Null);
        assert_eq!(unresolved["data"], Value::Null);
    }

    #[test]
    fn status_data_and_error_always_agree() {
        let success = Json::ok(
            "order.list",
            json!({"orders": []}),
            Warnings::new(),
            ExecutionMeta::read(None),
        );
        assert_eq!((success.status(), success.exit_code()), (Status::Ok, 0));
        assert_eq!(wire(&success)["status"], json!("ok"));

        for code in ERROR_CODES {
            let (name, exit) = error_contract(code);
            let failure = Json::error(
                Some("order.list"),
                code,
                Warnings::new(),
                ExecutionMeta::read(None),
            );
            let value = wire(&failure);
            assert_eq!(failure.status(), Status::Error);
            assert_eq!(value["status"], json!("error"));
            assert_eq!(value["data"], Value::Null);
            assert_eq!(value["error"]["code"], json!(name));
            assert_eq!(keys(&value["error"]), ["code", "message"]);
            assert_eq!(failure.exit_code(), exit);
            assert_eq!(code.exit_code(), exit);
        }
        assert_eq!(wire(&Status::Uncertain), json!("uncertain"));
    }

    #[test]
    fn error_codes_and_messages_follow_the_contract() {
        let names: Vec<Value> = ERROR_CODES.iter().map(wire).collect();
        let contract: Vec<Value> = ERROR_CODES
            .iter()
            .map(|code| json!(error_contract(*code).0))
            .collect();
        assert_eq!(names, contract);
        let mut messages: Vec<&str> = ERROR_CODES.iter().map(|code| code.message()).collect();
        for code in ERROR_CODES {
            assert_safe_message(code.message());
            let info = ErrorInfo::new(code);
            assert_eq!(info.code(), code);
            assert_eq!(
                wire(&info),
                json!({"code": error_contract(code).0, "message": code.message()})
            );
        }
        messages.sort_unstable();
        messages.dedup();
        assert_eq!(
            messages.len(),
            ERROR_CODES.len(),
            "messages must be distinct"
        );
        assert_eq!(ARGUMENT_ERROR_EXIT, 2);
        assert_eq!(RUNTIME_ERROR_EXIT, 1);
    }

    #[test]
    fn warnings_aggregate_by_code_and_never_serialize_as_null() {
        assert_eq!(wire(&Warnings::new()), json!([]));

        let mut warnings = Warnings::new();
        for _ in 0..3 {
            warnings.add(WarningCode::InvalidEventTimestamp);
        }
        warnings.add(WarningCode::CleanupIncomplete);
        assert!(warnings.contains(WarningCode::CleanupIncomplete));
        assert!(!warnings.is_empty());
        let expected: Vec<Value> = WARNING_CODES
            .iter()
            .map(|code| json!({"code": warning_contract(*code), "message": code.message()}))
            .collect();
        assert_eq!(wire(&warnings), Value::Array(expected));
        for code in WARNING_CODES {
            assert_safe_message(code.message());
        }

        // A cleanup warning never replaces the original failure.
        let mut cleanup = Warnings::new();
        cleanup.add(WarningCode::CleanupIncomplete);
        let failure = wire(&Json::error(
            Some("order.list"),
            ErrorCode::ReadTimeout,
            cleanup,
            ExecutionMeta::read(None),
        ));
        assert_eq!(failure["error"]["code"], json!("read_timeout"));
        assert_eq!(failure["warnings"][0]["code"], json!("cleanup_incomplete"));
    }

    #[test]
    fn stages_follow_the_data_model() {
        let read = wire(&ExecutionMeta::<Value>::read(None));
        assert_eq!(
            read,
            json!({
                "publication": "not_applicable",
                "daemon_acceptance": "not_applicable",
                "persistence": "not_applicable",
                "snapshot": null,
            })
        );
        let rejected = wire(&ExecutionMeta::<Value>::not_executed());
        assert_eq!(
            rejected,
            json!({
                "publication": "not_attempted",
                "daemon_acceptance": "unknown",
                "persistence": "not_attempted",
                "snapshot": null,
            })
        );
    }

    #[test]
    fn reserved_stage_sets_match_the_roadmap() {
        fn publication(stage: Publication) -> &'static str {
            match stage {
                Publication::NotAttempted => "not_attempted",
                Publication::Unknown => "unknown",
                Publication::Published => "published",
                Publication::NotApplicable => "not_applicable",
            }
        }
        fn acceptance(stage: DaemonAcceptance) -> &'static str {
            match stage {
                DaemonAcceptance::Unknown => "unknown",
                DaemonAcceptance::Accepted => "accepted",
                DaemonAcceptance::Rejected => "rejected",
                DaemonAcceptance::NotApplicable => "not_applicable",
            }
        }
        fn persistence(stage: Persistence) -> &'static str {
            match stage {
                Persistence::NotAttempted => "not_attempted",
                Persistence::Applied => "applied",
                Persistence::Failed => "failed",
                Persistence::NotApplicable => "not_applicable",
            }
        }
        for stage in [
            Publication::NotAttempted,
            Publication::Unknown,
            Publication::Published,
            Publication::NotApplicable,
        ] {
            assert_eq!(wire(&stage), json!(publication(stage)));
        }
        for stage in [
            DaemonAcceptance::Unknown,
            DaemonAcceptance::Accepted,
            DaemonAcceptance::Rejected,
            DaemonAcceptance::NotApplicable,
        ] {
            assert_eq!(wire(&stage), json!(acceptance(stage)));
        }
        for stage in [
            Persistence::NotAttempted,
            Persistence::Applied,
            Persistence::Failed,
            Persistence::NotApplicable,
        ] {
            assert_eq!(wire(&stage), json!(persistence(stage)));
        }
    }
}
