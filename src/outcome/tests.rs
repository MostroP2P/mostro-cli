//! Unit tests for the outcome envelope, its codes and its stages.

use serde_json::{json, Value};

use super::*;

const EXAMPLES: &str = include_str!("../../specs/001-order-list-json/contracts/examples.json");
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
const NEW_ERROR_CODES: [ErrorCode; 18] = [
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
    ErrorCode::OrderNotSaved,
    ErrorCode::StoreUnwritable,
    // And for the invoice commands.
    ErrorCode::OrderUnknown,
    // And for `message.list`.
    ErrorCode::StoreUnreadable,
    // And for `identity.init`.
    ErrorCode::IdentityExists,
    // And for `config.set`.
    ErrorCode::ConfigurationUnwritable,
    // And for `order.chat`.
    ErrorCode::CounterpartyUnknown,
    // And for `admin.order-info`.
    ErrorCode::AdminKeyMissing,
    ErrorCode::AdminKeyInvalid,
];
const NEW_WARNING_CODES: [WarningCode; 4] = [
    WarningCode::OrdersNotReturned,
    WarningCode::InvalidOrderTimestamp,
    WarningCode::LocalStatusStale,
    WarningCode::TradeKeysLimited,
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
        ErrorCode::OrderNotSaved => ("order_not_saved", 1),
        ErrorCode::StoreUnwritable => ("store_unwritable", 1),
        ErrorCode::OrderUnknown => ("order_unknown", 1),
        ErrorCode::StoreUnreadable => ("store_unreadable", 1),
        ErrorCode::IdentityExists => ("identity_exists", 1),
        ErrorCode::ConfigurationUnwritable => ("configuration_unwritable", 1),
        ErrorCode::CounterpartyUnknown => ("counterparty_unknown", 1),
        ErrorCode::AdminKeyMissing => ("admin_key_missing", 1),
        ErrorCode::AdminKeyInvalid => ("admin_key_invalid", 1),
    }
}

fn warning_contract(code: WarningCode) -> &'static str {
    match code {
        WarningCode::InvalidEventTimestamp => "invalid_event_timestamp",
        WarningCode::CleanupIncomplete => "cleanup_incomplete",
        WarningCode::OrdersNotReturned => "orders_not_returned",
        WarningCode::InvalidOrderTimestamp => "invalid_order_timestamp",
        WarningCode::LocalStatusStale => "local_status_stale",
        WarningCode::TradeKeysLimited => "trade_keys_limited",
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
fn only_pow_required_carries_the_work_the_node_asks_and_the_configured_floor() {
    let unsent = || ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown);
    let short = PowShortfall::new(21, 0);
    let envelope = Envelope::<Value, Value>::error(
        Some("order.new"),
        ErrorCode::PowRequired,
        Warnings::new(),
        unsent(),
    )
    .with_work(Some(short));
    let value = wire(&envelope);
    assert_eq!(
        keys(&value["error"]),
        ["code", "configured", "message", "required"]
    );
    assert_eq!(value["error"]["required"], json!(21));
    assert_eq!(value["error"]["configured"], json!(0));
    assert_eq!(envelope.work(), Some(short));
    assert_eq!(envelope.exit_code(), 1);

    // On the timeout path too.
    let uncertain = Envelope::<Value, Value>::uncertain(
        "order.new",
        ErrorCode::PowRequired,
        None,
        Warnings::new(),
        ExecutionMeta::request(Publication::Published, DaemonAcceptance::Unknown),
    )
    .with_work(Some(PowShortfall::new(12, 6)));
    assert_eq!(wire(&uncertain)["error"]["required"], json!(12));
    assert_eq!(wire(&uncertain)["error"]["configured"], json!(6));

    // Nothing to attach, or any other code: the error is unchanged.
    let bare = Envelope::<Value, Value>::error(
        Some("order.new"),
        ErrorCode::PowRequired,
        Warnings::new(),
        unsent(),
    )
    .with_work(None);
    assert_eq!(keys(&wire(&bare)["error"]), ["code", "message"]);
    assert_eq!(bare.work(), None);
    for code in ERROR_CODES.iter().chain(&NEW_ERROR_CODES) {
        let other =
            Envelope::<Value, Value>::error(Some("order.new"), *code, Warnings::new(), unsent())
                .with_work(Some(short));
        if *code != ErrorCode::PowRequired {
            assert_eq!(
                keys(&wire(&other)["error"]),
                ["code", "message"],
                "{code:?}"
            );
            assert_eq!(other.work(), None);
        }
    }
}

#[test]
fn request_stages_use_the_existing_enums_and_never_claim_a_local_write() {
    let cases = [
        (
            ExecutionMeta::<Value>::request(Publication::NotAttempted, DaemonAcceptance::Unknown),
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
