//! Machine-readable outcome envelope, contract version 1.
//!
//! An [`Envelope`] can only be built through its constructors, so status,
//! data and error always agree. Every key is serialized, as null when empty.
//! Error and warning messages are fixed prose chosen by their code; no
//! caller-provided text reaches the wire.

use schemars::JsonSchema;
use serde::Serialize;

pub use codes::{ErrorCode, ErrorInfo, Warning, WarningCode, Warnings};

mod codes;
pub mod info;
pub mod new;
pub mod order;
mod rules;
pub mod schema;

/// Contract version, independent of the executable version.
pub const SCHEMA_VERSION: &str = "1";
/// Exit status for argument and unsupported-output errors.
pub const ARGUMENT_ERROR_EXIT: i32 = 2;
/// Exit status for configuration, read and internal errors.
pub const RUNTIME_ERROR_EXIT: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Error,
    /// The request may have reached the node, or the node accepted it and the
    /// local write failed. Never `ok`. Only commands that publish a write emit
    /// it, through [`Envelope::uncertain`].
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Publication {
    NotAttempted,
    Unknown,
    Published,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DaemonAcceptance {
    Unknown,
    Accepted,
    Rejected,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    NotAttempted,
    Applied,
    Failed,
    NotApplicable,
}

/// Execution stages and optional snapshot evidence of one outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
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

    /// A command that publishes one request and writes nothing locally: the
    /// given publication and node-acceptance stages, persistence not applicable.
    pub fn request(publication: Publication, daemon_acceptance: DaemonAcceptance) -> Self {
        Self {
            publication,
            daemon_acceptance,
            persistence: Persistence::NotApplicable,
            snapshot: None,
        }
    }

    /// A command that publishes one request and writes locally: `persistence`
    /// says whether the write the outcome calls for was applied or failed.
    pub fn write(
        publication: Publication,
        daemon_acceptance: DaemonAcceptance,
        persistence: Persistence,
    ) -> Self {
        Self {
            publication,
            daemon_acceptance,
            persistence,
            snapshot: None,
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

    /// The request may have reached the node (or the node accepted it and the
    /// local write failed): never `ok`, never retried by the program. `data`
    /// is what the node answered, when it did.
    pub fn uncertain(
        command: &'static str,
        code: ErrorCode,
        data: Option<D>,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: Some(command),
            status: Status::Uncertain,
            data,
            error: Some(ErrorInfo::new(code)),
            warnings,
            meta,
        }
    }

    /// A refusal by the node: `node_refused` with its reason when that is a
    /// snake-case word. Data is null, as for every error.
    pub fn refused(
        command: Option<&'static str>,
        reason: &str,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        let mut envelope = Self::error(command, ErrorCode::NodeRefused, warnings, meta);
        envelope.error = envelope.error.take().map(|error| error.with_reason(reason));
        envelope
    }

    /// Record `extra` as well, still one warning per code.
    pub fn add_warnings(&mut self, extra: &Warnings) {
        self.warnings.merge(extra);
    }

    /// The node's refusal reason, only on `node_refused`.
    pub fn reason(&self) -> Option<&str> {
        self.error.as_ref().and_then(ErrorInfo::reason)
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

    pub fn snapshot(&self) -> Option<&S> {
        self.meta.snapshot.as_ref()
    }

    pub fn error_code(&self) -> Option<ErrorCode> {
        self.error.as_ref().map(ErrorInfo::code)
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
        self.error
            .as_ref()
            .map_or(0, |error| error.code().exit_code())
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
    /// Codes added for `order.info`; they belong to that command's schema only.
    const NEW_ERROR_CODES: [ErrorCode; 12] = [
        ErrorCode::IdentityMissing,
        ErrorCode::IdentityUnreadable,
        ErrorCode::ProtocolUnsupported,
        ErrorCode::PowRequired,
        ErrorCode::RequestRejected,
        ErrorCode::ReplyTimeout,
        ErrorCode::NodeRefused,
        ErrorCode::ReplyMalformed,
        // And for `order.new`.
        ErrorCode::OrderDeclined,
        ErrorCode::BondRequired,
        ErrorCode::OrderNotSaved,
        ErrorCode::StoreUnwritable,
    ];
    const NEW_WARNING_CODES: [WarningCode; 2] = [
        WarningCode::OrdersNotReturned,
        WarningCode::InvalidOrderTimestamp,
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
            ErrorCode::IdentityMissing => ("identity_missing", 1),
            ErrorCode::IdentityUnreadable => ("identity_unreadable", 1),
            ErrorCode::ProtocolUnsupported => ("protocol_unsupported", 1),
            ErrorCode::PowRequired => ("pow_required", 1),
            ErrorCode::RequestRejected => ("request_rejected", 1),
            ErrorCode::ReplyTimeout => ("reply_timeout", 1),
            ErrorCode::NodeRefused => ("node_refused", 1),
            ErrorCode::ReplyMalformed => ("reply_malformed", 1),
            ErrorCode::OrderDeclined => ("order_declined", 1),
            ErrorCode::BondRequired => ("bond_required", 1),
            ErrorCode::OrderNotSaved => ("order_not_saved", 1),
            ErrorCode::StoreUnwritable => ("store_unwritable", 1),
        }
    }

    fn warning_contract(code: WarningCode) -> &'static str {
        match code {
            WarningCode::InvalidEventTimestamp => "invalid_event_timestamp",
            WarningCode::CleanupIncomplete => "cleanup_incomplete",
            WarningCode::OrdersNotReturned => "orders_not_returned",
            WarningCode::InvalidOrderTimestamp => "invalid_order_timestamp",
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

    #[test]
    fn the_pilot_code_sets_are_frozen() {
        assert_eq!(ErrorCode::PILOT, ERROR_CODES);
        assert_eq!(WarningCode::PILOT, WARNING_CODES);
    }

    #[test]
    fn added_codes_follow_the_contract_and_stay_out_of_the_pilot_set() {
        let mut messages: Vec<&str> = ERROR_CODES
            .iter()
            .chain(&NEW_ERROR_CODES)
            .map(|code| code.message())
            .collect();
        for code in NEW_ERROR_CODES {
            let (name, exit) = error_contract(code);
            assert_eq!(wire(&code), json!(name));
            assert_eq!((code.exit_code(), exit), (1, 1), "{name}");
            assert_safe_message(code.message());
            assert!(!ErrorCode::PILOT.contains(&code), "{name}");
        }
        for code in NEW_WARNING_CODES {
            assert_eq!(wire(&code), json!(warning_contract(code)));
            assert_safe_message(code.message());
            assert!(!WarningCode::PILOT.contains(&code));
        }
        messages.sort_unstable();
        messages.dedup();
        assert_eq!(messages.len(), ERROR_CODES.len() + NEW_ERROR_CODES.len());
    }

    #[test]
    fn only_a_node_refusal_carries_a_reason_and_it_is_snake_case() {
        let refused = Envelope::<Value, Value>::refused(
            Some("order.info"),
            "too_many_requests",
            Warnings::new(),
            ExecutionMeta::request(Publication::Published, DaemonAcceptance::Rejected),
        );
        let value = wire(&refused);
        assert_eq!(keys(&value["error"]), ["code", "message", "reason"]);
        assert_eq!(value["error"]["code"], json!("node_refused"));
        assert_eq!(value["error"]["reason"], json!("too_many_requests"));
        assert_eq!(refused.exit_code(), 1);
        assert_eq!(refused.error_code(), Some(ErrorCode::NodeRefused));

        // A reason that is not a snake-case word is dropped, never echoed.
        for bad in ["", "Too Many", "x\ny", "http://evil", "has-dash", "9lives"] {
            let refused = Envelope::<Value, Value>::refused(
                Some("order.info"),
                bad,
                Warnings::new(),
                ExecutionMeta::request(Publication::Published, DaemonAcceptance::Rejected),
            );
            let value = wire(&refused);
            assert_eq!(keys(&value["error"]), ["code", "message"], "{bad:?}");
            assert!(
                !value.to_string().contains(bad) || bad.is_empty(),
                "{bad:?}"
            );
        }

        // Every other error is without a reason.
        for code in ERROR_CODES.iter().chain(&NEW_ERROR_CODES) {
            assert_eq!(keys(&wire(&ErrorInfo::new(*code))), ["code", "message"]);
        }
    }

    #[test]
    fn request_stages_use_the_existing_enums_and_never_claim_a_local_write() {
        let cases = [
            (
                ExecutionMeta::<Value>::request(
                    Publication::NotAttempted,
                    DaemonAcceptance::Unknown,
                ),
                ["not_attempted", "unknown"],
            ),
            (
                ExecutionMeta::request(Publication::Published, DaemonAcceptance::Accepted),
                ["published", "accepted"],
            ),
            (
                ExecutionMeta::request(Publication::Published, DaemonAcceptance::Rejected),
                ["published", "rejected"],
            ),
            (
                ExecutionMeta::request(Publication::Published, DaemonAcceptance::Unknown),
                ["published", "unknown"],
            ),
            (
                ExecutionMeta::request(Publication::Unknown, DaemonAcceptance::Unknown),
                ["unknown", "unknown"],
            ),
        ];
        for (meta, [publication, daemon]) in cases {
            let value = wire(&meta);
            assert_eq!(value["publication"], json!(publication));
            assert_eq!(value["daemon_acceptance"], json!(daemon));
            assert_eq!(value["persistence"], json!("not_applicable"));
            assert_eq!(value["snapshot"], Value::Null);
        }
    }

    #[test]
    fn only_the_uncertain_constructor_emits_uncertain_and_never_exits_zero() {
        let uncertain = Json::uncertain(
            "order.new",
            ErrorCode::ReplyTimeout,
            None,
            Warnings::new(),
            ExecutionMeta::write(
                Publication::Published,
                DaemonAcceptance::Unknown,
                Persistence::Applied,
            ),
        );
        assert_eq!(uncertain.status(), Status::Uncertain);
        assert_eq!(uncertain.exit_code(), 1);
        assert_eq!(uncertain.error_code(), Some(ErrorCode::ReplyTimeout));
        assert!(uncertain.data().is_none());
        let wire = serde_json::to_value(&uncertain).unwrap();
        assert_eq!(wire["status"], "uncertain");
        assert_eq!(wire["meta"]["persistence"], "applied");
        assert_eq!(wire["meta"]["snapshot"], Value::Null);
    }

    #[test]
    fn no_other_constructor_emits_uncertain() {
        let all = ERROR_CODES.iter().chain(&NEW_ERROR_CODES);
        for code in all {
            let failure = Json::error(
                Some("order.info"),
                *code,
                Warnings::new(),
                ExecutionMeta::request(Publication::Published, DaemonAcceptance::Unknown),
            );
            assert_eq!(failure.status(), Status::Error);
        }
        let ok = Json::ok(
            "order.info",
            json!({}),
            Warnings::new(),
            ExecutionMeta::request(Publication::Published, DaemonAcceptance::Accepted),
        );
        assert_eq!(ok.status(), Status::Ok);
    }
}
