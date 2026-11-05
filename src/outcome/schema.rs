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
use super::{ErrorCode, ErrorInfo, ExecutionMeta, Warnings, SCHEMA_VERSION};

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

/// What a command's outcomes may contain.
pub struct OutcomeRules<'a> {
    /// Canonical name reported on success.
    pub command: &'static str,
    /// Codes that can fail this command after it resolves.
    pub errors: &'a [ErrorCode],
    /// Whether a success always carries snapshot evidence.
    pub snapshot_on_success: bool,
}

fn value<T: serde::Serialize>(item: &T) -> Value {
    serde_json::to_value(item).unwrap_or(Value::Null)
}

fn code_names(codes: &[ErrorCode]) -> Vec<Value> {
    codes.iter().map(value).collect()
}

/// `meta` with fixed stages, and the snapshot rule given.
fn meta_rule(meta: &Value, stages: [&str; 3], snapshot: Value) -> Value {
    let mut rule = meta.as_object().cloned().unwrap_or_default();
    rule.insert(
        "properties".into(),
        json!({
            "publication": {"const": stages[0]},
            "daemon_acceptance": {"const": stages[1]},
            "persistence": {"const": stages[2]},
            "snapshot": snapshot,
        }),
    );
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
    let read = ["not_applicable"; 3];
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
            "data": data,
            "error": {"type": "null"},
            "meta": meta_rule(&meta, read, ok_snapshot),
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
                "properties": {"code": {"enum": code_names(rules.errors)}},
            },
            "meta": meta_rule(&meta, read, failure_snapshot),
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
                ["not_attempted", "unknown", "not_attempted"],
                json!({"type": "null"}),
            ),
        },
    });

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
            "status": {"enum": ["ok", "error"]},
            "warnings": warnings,
            "meta": meta,
        }),
    );
    document.insert("oneOf".into(), json!([success, failure, unresolved]));
    document.insert(
        "$defs".into(),
        Value::Object(generator.take_definitions(true)),
    );
    document
}

// One checker, shared with the integration tests that validate the fixtures.
#[cfg(test)]
#[path = "../../tests/common/schema_check.rs"]
mod schema_check;

#[cfg(test)]
mod tests {
    use super::schema_check::conforms;
    use super::*;
    use crate::outcome::order::{ListData, ListOrdersInput, Snapshot};
    use crate::outcome::{Envelope, ExecutionMeta, Warnings};
    use mostro_core::order::{Kind, SmallOrder, Status};
    use nostr_sdk::prelude::{Keys, Timestamp};
    use regex::Regex;
    use uuid::Uuid;

    const ERRORS: [ErrorCode; 2] = [ErrorCode::ReadTimeout, ErrorCode::InternalError];

    fn listing() -> Value {
        Value::Object(output_schema::<ListData, Snapshot>(&OutcomeRules {
            command: "order.list",
            errors: &ERRORS,
            snapshot_on_success: true,
        }))
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
        let schema = Value::Object(output_schema::<ListData, Snapshot>(&OutcomeRules {
            command: "order.list",
            errors: &ERRORS,
            snapshot_on_success: false,
        }));
        assert!(!conforms(&schema, &schema, &success()));
        let mut no_snapshot = success();
        no_snapshot["meta"]["snapshot"] = Value::Null;
        assert!(conforms(&schema, &schema, &no_snapshot));
    }
}
