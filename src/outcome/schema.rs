//! Draft 2020-12 output schemas, generated from the owned outcome types.
//!
//! Types derive their shape with schemars; this module adds what a derive
//! cannot say: wire patterns for decimal and time strings, and the
//! command-specific constraints of an envelope (command, status, data, error
//! and stage combinations). Documents are plain JSON objects.

use schemars::generate::SchemaSettings;
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

use super::order::{SIDES, STATUSES};
use super::{ErrorCode, ErrorInfo, ExecutionMeta, WarningCode, Warnings, SCHEMA_VERSION};

use super::rules::{code_names, scoped_codes, value};
pub use super::rules::{OutcomeRules, Stages};

pub type SchemaDocument = Map<String, Value>;

/// The dialect every generated document declares.
pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Canonical decimal text: `0`, or an optional minus and a nonzero-leading
/// digit string. The signed 64-bit range cannot be expressed as a pattern and
/// is enforced by the producer.
pub const DECIMAL_PATTERN: &str = "^(0|-?[1-9][0-9]*)$";
pub const UUID_PATTERN: &str = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$";
pub const PUBKEY_PATTERN: &str = "^[0-9a-f]{64}$";
pub const RFC3339_PATTERN: &str =
    "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]Z$";

/// A refusal reason: a snake-case word, at most 64 characters.
pub const REASON_PATTERN: &str = "^[a-z][a-z0-9_]{0,63}$";

/// Longest signed 64-bit decimal, `-9223372036854775808`.
const DECIMAL_MAX_LENGTH: u32 = 20;

pub fn decimal(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": DECIMAL_PATTERN, "maxLength": DECIMAL_MAX_LENGTH})
}

pub fn nullable_decimal(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": ["string", "null"],
        "pattern": DECIMAL_PATTERN,
        "maxLength": DECIMAL_MAX_LENGTH,
    })
}

pub fn uuid(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": UUID_PATTERN})
}

pub fn pubkey_hex(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": PUBKEY_PATTERN})
}

pub fn rfc3339(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": RFC3339_PATTERN})
}

pub fn nullable_rfc3339(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "pattern": RFC3339_PATTERN})
}

pub fn nullable_pubkey(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "pattern": PUBKEY_PATTERN})
}

pub fn uuid_list(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "array", "items": {"type": "string", "pattern": UUID_PATTERN}})
}

pub fn nullable_status(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "enum": status_with_null()})
}

fn status_with_null() -> Vec<Value> {
    STATUSES
        .iter()
        .map(|status| json!(status))
        .chain([Value::Null])
        .collect()
}

pub fn side(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "enum": SIDES})
}

pub fn nullable_side(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "enum": [SIDES[0], SIDES[1], null]})
}

pub fn status(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "enum": STATUSES})
}

pub fn per_relay_limit(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "const": 50})
}

pub fn max_unique_events(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "const": 10_000})
}

pub fn positive_count(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "minimum": 1})
}

pub fn count(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "minimum": 0})
}

pub fn received_events(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "minimum": 0, "maximum": 10_000})
}

pub fn never_complete_history(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "boolean", "const": false})
}

/// `meta` limited to the allowed stage combinations and the snapshot rule given.
fn meta_rule(meta: &Value, combos: &[[&str; 3]], snapshot: Value) -> Value {
    let stages = |combo: &[&str; 3]| {
        json!({
            "publication": {"const": combo[0]},
            "daemon_acceptance": {"const": combo[1]},
            "persistence": {"const": combo[2]},
        })
    };
    let mut rule = meta.as_object().cloned().unwrap_or_default();
    if let [only] = combos {
        let mut properties = stages(only);
        properties["snapshot"] = snapshot;
        rule.insert("properties".into(), properties);
    } else {
        rule.insert("properties".into(), json!({ "snapshot": snapshot }));
        let alternatives: Vec<Value> = combos
            .iter()
            .map(|combo| json!({ "properties": stages(combo) }))
            .collect();
        rule.insert("anyOf".into(), Value::Array(alternatives));
    }
    Value::Object(rule)
}

/// The output schema of one command: an envelope whose success carries `D`
/// and whose failures carry no data. `S` is the snapshot evidence type.
pub fn output_schema<D: JsonSchema, S: JsonSchema>(rules: &OutcomeRules) -> SchemaDocument {
    let mut generator = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    let data = value(&generator.subschema_for::<D>());
    let snapshot = value(&generator.subschema_for::<S>());
    let meta = value(&generator.subschema_for::<ExecutionMeta<S>>());
    let error = value(&generator.subschema_for::<ErrorInfo>());
    let warnings = value(&generator.subschema_for::<Warnings>());
    let combos = rules.combos();
    let ok_snapshot = if rules.snapshot_on_success {
        snapshot.clone()
    } else {
        json!({"type": "null"})
    };
    let failure_snapshot = json!({"anyOf": [snapshot, {"type": "null"}]});
    let command = rules.command;

    let success = json!({
        "title": "success",
        "properties": {
            "command": {"const": command},
            "status": {"const": "ok"},
            "data": data.clone(),
            "error": {"type": "null"},
            "meta": meta_rule(&meta, &combos.success, ok_snapshot),
        },
    });
    let failure = json!({
        "title": "failure of the resolved command",
        "properties": {
            "command": {"const": command},
            "status": {"const": "error"},
            "data": {"type": "null"},
            "error": {
                "allOf": [error.clone()],
                "properties": failure_error_properties(rules),
            },
            "meta": meta_rule(&meta, &combos.failure, failure_snapshot.clone()),
        },
    });
    // The request may have reached the node: data only when the node answered.
    let uncertain = json!({
        "title": "uncertain outcome of the resolved command",
        "properties": {
            "command": {"const": command},
            "status": {"const": "uncertain"},
            "data": {"anyOf": [data, {"type": "null"}]},
            "error": {
                "allOf": [error.clone()],
                "properties": {"code": {"enum": code_names(rules.uncertain)}},
            },
            "meta": meta_rule(&meta, &combos.uncertain, failure_snapshot),
        },
    });
    let unresolved = json!({
        "title": "failure before the command was resolved",
        "properties": {
            "command": {"type": "null"},
            "status": {"const": "error"},
            "data": {"type": "null"},
            "error": {
                "allOf": [error],
                "properties": {"code": {"const": ErrorCode::InvalidArguments}},
            },
            "meta": meta_rule(
                &meta,
                &[["not_attempted", "unknown", "not_attempted"]],
                json!({"type": "null"}),
            ),
        },
    });

    let mut outcomes = vec![success, failure];
    let mut statuses = vec!["ok", "error"];
    if !rules.uncertain.is_empty() {
        outcomes.push(uncertain);
        statuses.push("uncertain");
    }
    outcomes.push(unresolved);

    let mut document = Map::new();
    document.insert("$schema".into(), json!(DIALECT));
    document.insert("title".into(), json!(format!("{command} outcome")));
    document.insert("type".into(), json!("object"));
    document.insert(
        "required".into(),
        json!([
            "schema_version",
            "command",
            "status",
            "data",
            "error",
            "warnings",
            "meta"
        ]),
    );
    document.insert(
        "properties".into(),
        json!({
            "schema_version": {"const": SCHEMA_VERSION},
            "command": {"type": ["string", "null"]},
            "status": {"enum": statuses},
            "warnings": warnings,
            "meta": meta,
        }),
    );
    document.insert("oneOf".into(), Value::Array(outcomes));
    let mut definitions = generator.take_definitions(true);
    scope_enum(
        &mut definitions,
        "ErrorCode",
        scoped_codes(&ErrorCode::PILOT, &[rules.errors, rules.uncertain].concat()),
    );
    scope_enum(
        &mut definitions,
        "WarningCode",
        scoped_codes(&WarningCode::PILOT, rules.warnings),
    );
    document.insert("$defs".into(), Value::Object(definitions));
    document
}

/// The properties a failure of the resolved command may have on `error`.
fn failure_error_properties(rules: &OutcomeRules) -> Value {
    let mut properties = json!({"code": {"enum": code_names(rules.errors)}});
    if rules.reason {
        properties["reason"] = json!({
            "type": "string",
            "pattern": REASON_PATTERN,
            "description": "The node's refusal reason; present only on node_refused.",
        });
    }
    properties
}

/// Replace the `enum` of the shared definition `name`, when it exists.
fn scope_enum(definitions: &mut Map<String, Value>, name: &str, allowed: Value) {
    if let Some(Value::Object(definition)) = definitions.get_mut(name) {
        definition.insert("enum".into(), allowed);
    }
}

// One checker, shared with the integration tests that validate the fixtures.
#[cfg(test)]
#[path = "../../tests/common/schema_check.rs"]
pub(super) mod schema_check;

#[cfg(test)]
mod tests {
    use super::schema_check::conforms;
    use super::*;
    use crate::outcome::info::{OrderInfoData, OrderInfoInput};
    use crate::outcome::order::{ListData, ListOrdersInput, Snapshot};
    use crate::outcome::{
        DaemonAcceptance, Envelope, ExecutionMeta, Publication, WarningCode, Warnings,
    };
    use mostro_core::order::{Kind, SmallOrder, Status};
    use nostr_sdk::prelude::{Keys, Timestamp};
    use regex::Regex;
    use uuid::Uuid;

    const ERRORS: [ErrorCode; 2] = [ErrorCode::ReadTimeout, ErrorCode::InternalError];

    fn listing() -> Value {
        Value::Object(output_schema::<ListData, Snapshot>(&OutcomeRules::read(
            "order.list",
            &ERRORS,
            true,
        )))
    }

    fn defs(schema: &Value) -> &Value {
        &schema["$defs"]
    }

    fn strings(value: &Value) -> Vec<&str> {
        value
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    fn order() -> SmallOrder {
        SmallOrder::new(
            Some(Uuid::new_v4()),
            Some(Kind::Sell),
            Some(Status::Pending),
            15_000,
            "USD".to_string(),
            Some(10),
            Some(20),
            15,
            "bank transfer".to_string(),
            -3,
            None,
            None,
            None,
            Some(1_791_504_000),
            None,
        )
    }

    fn snapshot() -> Snapshot {
        Snapshot::new(
            Timestamp::from(1_791_504_000),
            Timestamp::from(1_792_108_800),
            50,
            10_000,
            2,
            2,
            1,
        )
    }

    fn success() -> Value {
        let filters = ListOrdersInput::parse(None, None, None).unwrap();
        let node = Keys::generate().public_key();
        let (data, warnings) = ListData::from_selected(&node, filters, &[order()]).unwrap();
        let outcome = Envelope::ok(
            "order.list",
            data,
            warnings,
            ExecutionMeta::read(Some(snapshot())),
        );
        serde_json::to_value(outcome).unwrap()
    }

    fn failure(
        command: Option<&'static str>,
        code: ErrorCode,
        meta: ExecutionMeta<Snapshot>,
    ) -> Value {
        let outcome = Envelope::<ListData, Snapshot>::error(command, code, Warnings::new(), meta);
        serde_json::to_value(outcome).unwrap()
    }

    #[test]
    fn the_document_declares_its_dialect_and_every_envelope_key() {
        let schema = listing();
        assert_eq!(schema["$schema"], DIALECT);
        assert_eq!(schema["type"], "object");
        assert_eq!(
            strings(&schema["required"]),
            [
                "schema_version",
                "command",
                "status",
                "data",
                "error",
                "warnings",
                "meta"
            ]
        );
        assert_eq!(schema["properties"]["schema_version"]["const"], "1");
        assert_eq!(
            schema["properties"]["status"]["enum"],
            json!(["ok", "error"])
        );
        assert!(
            schema.get("additionalProperties").is_none(),
            "additive fields stay compatible"
        );
    }

    #[test]
    fn real_outcomes_conform_to_their_command_schema() {
        let schema = listing();
        let read = ExecutionMeta::read(None);
        let cases = [
            success(),
            failure(Some("order.list"), ErrorCode::ReadTimeout, read),
            failure(
                Some("order.list"),
                ErrorCode::InternalError,
                ExecutionMeta::read(Some(snapshot())),
            ),
            failure(
                None,
                ErrorCode::InvalidArguments,
                ExecutionMeta::not_executed(),
            ),
        ];
        for case in cases {
            assert!(conforms(&schema, &schema, &case), "{case}");
        }
    }

    #[test]
    fn contradictory_or_malformed_outcomes_do_not_conform() {
        let schema = listing();
        let read = || ExecutionMeta::read(None);
        let mut rejected = vec![
            // A code this command cannot produce, and a legacy-only code.
            failure(Some("order.list"), ErrorCode::RelayReadFailed, read()),
            failure(Some("order.list"), ErrorCode::OutputNotSupported, read()),
            // A different command, or an unresolved one with a non-argument code.
            failure(Some("order.new"), ErrorCode::ReadTimeout, read()),
            failure(None, ErrorCode::ReadTimeout, ExecutionMeta::not_executed()),
            // Stage sets belong to their branch.
            failure(None, ErrorCode::InvalidArguments, read()),
            failure(
                Some("order.list"),
                ErrorCode::ReadTimeout,
                ExecutionMeta::not_executed(),
            ),
        ];
        let mutate = |edit: &dyn Fn(&mut Value)| {
            let mut outcome = success();
            edit(&mut outcome);
            outcome
        };
        rejected.extend([
            mutate(&|o| o["status"] = json!("uncertain")),
            mutate(&|o| o["status"] = json!("error")),
            mutate(&|o| o["command"] = json!("order.new")),
            mutate(&|o| o["command"] = Value::Null),
            mutate(&|o| o["data"] = Value::Null),
            mutate(&|o| o["error"] = json!({"code": "read_timeout", "message": "x"})),
            mutate(&|o| o["schema_version"] = json!("2")),
            mutate(&|o| o["meta"]["snapshot"] = Value::Null),
            mutate(&|o| o["meta"]["publication"] = json!("published")),
            mutate(&|o| {
                o.as_object_mut().unwrap().remove("warnings");
            }),
            mutate(&|o| o["data"]["orders"][0]["sats_amount"] = json!("-0")),
            mutate(&|o| o["data"]["orders"][0]["fiat_amount"] = json!(15)),
            mutate(&|o| o["data"]["orders"][0]["status"] = json!("uncertain")),
            mutate(&|o| o["data"]["orders"][0]["id"] = json!("not-a-uuid")),
            mutate(&|o| o["data"]["orders"][0]["created_at"] = json!("2026-10-16T00:00:00+02:00")),
            mutate(&|o| {
                o["data"]["orders"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("fiat_amount_min");
            }),
            mutate(&|o| o["data"]["node_pubkey"] = json!("ABC")),
            mutate(&|o| o["meta"]["snapshot"]["complete_history"] = json!(true)),
            mutate(&|o| o["meta"]["snapshot"]["until"] = json!("2026-10-16T00:00:00Z")),
        ]);
        for case in rejected {
            assert!(!conforms(&schema, &schema, &case), "accepted: {case}");
        }
    }

    #[test]
    fn nullable_fields_are_required_and_typed_with_null() {
        let schema = listing();
        let order = &defs(&schema)["PublicOrder"];
        let keys = [
            "id",
            "side",
            "status",
            "sats_amount",
            "fiat_currency",
            "fiat_amount",
            "fiat_amount_min",
            "fiat_amount_max",
            "payment_method",
            "premium_percent",
            "created_at",
            "created_at_unix",
        ];
        assert_eq!(strings(&order["required"]), keys);
        for key in ["fiat_amount_min", "fiat_amount_max", "created_at"] {
            assert!(
                strings(&order["properties"][key]["type"]).contains(&"null"),
                "{key}"
            );
        }
        for key in [
            "id",
            "side",
            "status",
            "sats_amount",
            "fiat_amount",
            "created_at_unix",
        ] {
            assert_eq!(order["properties"][key]["type"], "string", "{key}");
        }
        let filters = &defs(&schema)["ListOrdersInput"];
        assert_eq!(
            strings(&filters["required"]),
            ["side", "currency", "status"]
        );
        assert!(strings(&filters["properties"]["side"]["type"]).contains(&"null"));
        assert!(strings(&filters["properties"]["currency"]["type"]).contains(&"null"));
        assert_eq!(
            defs(&schema)["ListData"]["required"],
            json!(["node_pubkey", "filters", "orders"])
        );
    }

    #[test]
    fn decimal_pattern_accepts_only_canonical_integers() {
        let pattern = Regex::new(DECIMAL_PATTERN).unwrap();
        for good in [
            "0",
            "7",
            "-7",
            "9223372036854775807",
            "-9223372036854775808",
        ] {
            assert!(pattern.is_match(good), "{good}");
        }
        for bad in [
            "", "-0", "+1", "01", "-01", "1.0", "1e3", " 1", "1 ", "0x1", "--1",
        ] {
            assert!(!pattern.is_match(bad), "{bad:?}");
        }
        assert_eq!("-9223372036854775808".len() as u32, DECIMAL_MAX_LENGTH);
    }

    #[test]
    fn identity_and_time_patterns_follow_the_wire_forms() {
        let uuid = Regex::new(UUID_PATTERN).unwrap();
        assert!(uuid.is_match("61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0"));
        assert!(!uuid.is_match("61BD0415-A9BC-4C9C-A1FC-BB2C305CF3B0"));
        assert!(!uuid.is_match("61bd0415a9bc4c9ca1fcbb2c305cf3b0"));
        let key = Regex::new(PUBKEY_PATTERN).unwrap();
        assert!(key.is_match(&"a".repeat(64)) && !key.is_match(&"A".repeat(64)));
        assert!(!key.is_match(&"a".repeat(63)));
        let time = Regex::new(RFC3339_PATTERN).unwrap();
        assert!(time.is_match("2026-10-16T00:00:00Z") && time.is_match("0000-01-01T00:00:00Z"));
        for bad in [
            "2026-10-16T00:00:00+02:00",
            "2026-10-16 00:00:00Z",
            "2026-13-01T00:00:00Z",
            "10000-01-01T00:00:00Z",
            "2026-10-16T00:00:00z",
        ] {
            assert!(!time.is_match(bad), "{bad}");
        }
    }

    #[test]
    fn enums_match_the_contract() {
        let schema = listing();
        assert_eq!(
            strings(&defs(&schema)["ErrorCode"]["enum"]),
            [
                "invalid_arguments",
                "output_not_supported",
                "configuration_missing",
                "configuration_invalid",
                "relay_connection_failed",
                "relay_read_failed",
                "read_timeout",
                "read_limit_exceeded",
                "internal_error",
            ]
        );
        assert_eq!(
            strings(&defs(&schema)["WarningCode"]["enum"]),
            ["invalid_event_timestamp", "cleanup_incomplete"]
        );
        let order = &defs(&schema)["PublicOrder"]["properties"];
        assert_eq!(strings(&order["side"]["enum"]), SIDES);
        assert_eq!(strings(&order["status"]["enum"]), STATUSES);
        assert_eq!(
            strings(&defs(&schema)["Publication"]["enum"]),
            ["not_attempted", "unknown", "published", "not_applicable"]
        );
        assert_eq!(
            defs(&schema)["Snapshot"]["properties"]["until"]["type"],
            "null"
        );
    }

    #[test]
    fn a_command_without_snapshot_evidence_requires_a_null_one() {
        let schema = Value::Object(output_schema::<ListData, Snapshot>(&OutcomeRules::read(
            "order.list",
            &ERRORS,
            false,
        )));
        assert!(!conforms(&schema, &schema, &success()));
        let mut no_snapshot = success();
        no_snapshot["meta"]["snapshot"] = Value::Null;
        assert!(conforms(&schema, &schema, &no_snapshot));
    }

    // --- The request vocabulary of `order.info` ----------------------------

    const INFO_ERRORS: [ErrorCode; 13] = [
        ErrorCode::InvalidArguments,
        ErrorCode::ConfigurationMissing,
        ErrorCode::ConfigurationInvalid,
        ErrorCode::RelayConnectionFailed,
        ErrorCode::RelayReadFailed,
        ErrorCode::InternalError,
        ErrorCode::IdentityMissing,
        ErrorCode::IdentityUnreadable,
        ErrorCode::ProtocolUnsupported,
        ErrorCode::PowRequired,
        ErrorCode::RequestRejected,
        ErrorCode::ReplyTimeout,
        ErrorCode::NodeRefused,
    ];
    // `reply_malformed` is added in the list below so the set stays readable.
    const INFO_WARNINGS: [WarningCode; 2] = [
        WarningCode::OrdersNotReturned,
        WarningCode::InvalidOrderTimestamp,
    ];

    fn info_codes() -> Vec<ErrorCode> {
        let mut codes = INFO_ERRORS.to_vec();
        codes.push(ErrorCode::ReplyMalformed);
        codes
    }

    fn info_schema() -> Value {
        let codes = info_codes();
        Value::Object(output_schema::<OrderInfoData, ()>(&OutcomeRules::request(
            "order.info",
            &codes,
            &INFO_WARNINGS,
        )))
    }

    fn info_data() -> (OrderInfoData, Warnings) {
        let node = Keys::generate().public_key();
        let ids = [Uuid::from_u128(1), Uuid::from_u128(2)];
        let requested = OrderInfoInput::new(ids).unwrap();
        OrderInfoData::from_reply(
            &node,
            &requested,
            &[SmallOrder::new(
                Some(ids[0]),
                Some(Kind::Sell),
                Some(Status::Active),
                15_000,
                "USD".to_string(),
                None,
                None,
                15,
                "bank transfer".to_string(),
                1,
                Some(Keys::generate().public_key().to_hex()),
                None,
                None,
                Some(1_792_108_800),
                Some(1_792_195_200),
            )],
        )
    }

    fn info_ok(meta: ExecutionMeta<()>) -> Value {
        let (data, warnings) = info_data();
        serde_json::to_value(Envelope::ok("order.info", data, warnings, meta)).unwrap()
    }

    fn info_failure(code: ErrorCode, meta: ExecutionMeta<()>) -> Value {
        let envelope =
            Envelope::<OrderInfoData, ()>::error(Some("order.info"), code, Warnings::new(), meta);
        serde_json::to_value(envelope).unwrap()
    }

    fn request(publication: Publication, daemon: DaemonAcceptance) -> ExecutionMeta<()> {
        ExecutionMeta::request(publication, daemon)
    }

    #[test]
    fn a_request_command_lists_the_pilot_codes_plus_its_own_and_a_reason() {
        let schema = info_schema();
        let defs = defs(&schema);
        let names = |value: &Value| {
            strings(value)
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        let mut expected: Vec<String> = ErrorCode::PILOT
            .iter()
            .map(|c| {
                serde_json::to_value(c)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        expected.extend(
            [
                "identity_missing",
                "identity_unreadable",
                "protocol_unsupported",
                "pow_required",
                "request_rejected",
                "reply_timeout",
                "node_refused",
                "reply_malformed",
            ]
            .map(String::from),
        );
        assert_eq!(names(&defs["ErrorCode"]["enum"]), expected);
        assert_eq!(
            names(&defs["WarningCode"]["enum"]),
            [
                "invalid_event_timestamp",
                "cleanup_incomplete",
                "orders_not_returned",
                "invalid_order_timestamp"
            ]
        );
        let failure = &schema["oneOf"][1]["properties"]["error"]["properties"];
        assert_eq!(failure["reason"]["pattern"], REASON_PATTERN);
        // A read command has no reason and keeps the pilot sets.
        let read = listing();
        assert!(read["oneOf"][1]["properties"]["error"]["properties"]
            .get("reason")
            .is_none());
        assert_eq!(strings(&self::defs(&read)["ErrorCode"]["enum"]).len(), 9);
        // The data definitions are in the document.
        assert!(defs.get("OrderInfoData").is_some() && defs.get("OrderRecord").is_some());
    }

    #[test]
    fn real_request_outcomes_of_every_kind_conform() {
        let schema = info_schema();
        let cases = [
            info_ok(request(Publication::Published, DaemonAcceptance::Accepted)),
            info_failure(
                ErrorCode::IdentityMissing,
                request(Publication::NotAttempted, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::PowRequired,
                request(Publication::NotAttempted, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::PowRequired,
                request(Publication::Published, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::ReplyTimeout,
                request(Publication::Published, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::RequestRejected,
                request(Publication::Unknown, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::ReplyMalformed,
                request(Publication::Published, DaemonAcceptance::Unknown),
            ),
            serde_json::to_value(Envelope::<OrderInfoData, ()>::refused(
                Some("order.info"),
                "too_many_requests",
                Warnings::new(),
                request(Publication::Published, DaemonAcceptance::Rejected),
            ))
            .unwrap(),
            serde_json::to_value(Envelope::<OrderInfoData, ()>::error(
                None,
                ErrorCode::InvalidArguments,
                Warnings::new(),
                ExecutionMeta::not_executed(),
            ))
            .unwrap(),
        ];
        for case in cases {
            assert!(conforms(&schema, &schema, &case), "{case}");
        }
    }

    #[test]
    fn contradictory_request_outcomes_do_not_conform() {
        let schema = info_schema();
        let ok = || info_ok(request(Publication::Published, DaemonAcceptance::Accepted));
        let refused = || {
            serde_json::to_value(Envelope::<OrderInfoData, ()>::refused(
                Some("order.info"),
                "not_found",
                Warnings::new(),
                request(Publication::Published, DaemonAcceptance::Rejected),
            ))
            .unwrap()
        };
        let mut rejected = vec![
            // Read stages do not describe a request.
            info_ok(ExecutionMeta::read(None)),
            // A success needs an accepted request; a failure cannot be accepted.
            info_ok(request(Publication::Published, DaemonAcceptance::Unknown)),
            info_failure(
                ErrorCode::ReplyTimeout,
                request(Publication::Published, DaemonAcceptance::Accepted),
            ),
            info_failure(
                ErrorCode::ReplyTimeout,
                request(Publication::Published, DaemonAcceptance::NotApplicable),
            ),
            // Codes the command cannot emit.
            info_failure(
                ErrorCode::OutputNotSupported,
                request(Publication::NotAttempted, DaemonAcceptance::Unknown),
            ),
            info_failure(
                ErrorCode::ReadTimeout,
                request(Publication::Published, DaemonAcceptance::Unknown),
            ),
        ];
        let edit = |base: Value, change: &dyn Fn(&mut Value)| {
            let mut value = base;
            change(&mut value);
            value
        };
        rejected.extend([
            edit(ok(), &|o| o["status"] = json!("uncertain")),
            edit(ok(), &|o| o["meta"]["persistence"] = json!("applied")),
            edit(ok(), &|o| o["meta"]["snapshot"] = json!({})),
            edit(ok(), &|o| {
                o["data"]["not_returned"][0] = json!("not-a-uuid")
            }),
            edit(ok(), &|o| {
                o["data"]["orders"][0]["buyer_trade_pubkey"] = json!("ABC")
            }),
            edit(ok(), &|o| {
                o["data"]["orders"][0]["sats_amount"] = json!(15000)
            }),
            edit(ok(), &|o| {
                o["data"]["orders"][0]["status"] = json!("uncertain")
            }),
            edit(ok(), &|o| {
                o["data"]["orders"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expires_at");
            }),
            edit(ok(), &|o| o["data"] = Value::Null),
            edit(refused(), &|o| o["error"]["reason"] = json!("Not Found")),
            edit(refused(), &|o| o["error"]["reason"] = json!(7)),
        ]);
        for case in rejected {
            assert!(!conforms(&schema, &schema, &case), "accepted: {case}");
        }
    }
}
