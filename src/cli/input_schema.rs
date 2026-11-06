//! Input schemas of the machine-capable commands, read from the real Clap
//! grammar: one property per long option of the leaf command, with its type,
//! allowed values, case rule and help text. Root options are configuration,
//! not input, and are described by discovery's `required_configuration`.

use clap::{Arg, ArgAction, Command};
use serde_json::{json, Map, Value};

use super::args::REPLY_TIMEOUT_LONG;
use super::invocation::OUTPUT_LONG;
use super::registry::{CommandEntry, JsonSupport};
use super::stdin::{STDIN_OPTIONS, STDIN_VALUE};
use crate::outcome::order::ListOrdersInput;
use crate::outcome::schema::{SchemaDocument, DIALECT as SCHEMA_DIALECT};

/// The arguments of the command at `path`, read from Clap. Root options are
/// configuration, not input, and are described by `required_configuration`.
pub fn input_schema(root: &Command, path: &[&str]) -> Option<SchemaDocument> {
    let leaf = path
        .iter()
        .try_fold(root, |level, name| level.find_subcommand(name))?;
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in leaf.get_arguments().filter(|arg| is_input(arg)) {
        let name = arg.get_long()?;
        properties.insert(name.to_string(), property(arg));
        if arg.is_required_set() {
            required.push(json!(name));
        }
    }
    if path == ["order", "list"] {
        // The default lives in the shared validator, not in Clap.
        if let (Some(Value::Object(status)), Ok(filters)) = (
            properties.get_mut("status"),
            ListOrdersInput::parse(None, None, None),
        ) {
            status.insert("default".into(), json!(filters.status()));
        }
    }
    let mut document = Map::new();
    document.insert("$schema".into(), json!(SCHEMA_DIALECT));
    document.insert("type".into(), json!("object"));
    document.insert("properties".into(), Value::Object(properties));
    document.insert("required".into(), Value::Array(required));
    document.insert("additionalProperties".into(), json!(false));
    Some(document)
}

/// An option of the command itself: not hidden, not help or version, and not a
/// global option (`--output`, `--reply-timeout`), which discovery lists apart.
fn is_input(arg: &Arg) -> bool {
    !arg.is_hide_set()
        && !arg.is_global_set()
        && !matches!(arg.get_id().as_str(), "help" | "version" | OUTPUT_LONG)
        && arg.get_long().is_some()
}

/// The schema property of one option: its type, allowed values, case rule,
/// stdin marker and help text.
fn property(arg: &Arg) -> Value {
    let mut value = Map::new();
    if is_flag(arg) {
        // A flag takes no value: present means true.
        value.insert("type".into(), json!("boolean"));
    } else {
        value.insert("type".into(), json!("string"));
        let values: Vec<String> = arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_string())
            .collect();
        if !values.is_empty() {
            value.insert("enum".into(), json!(values));
        }
        if arg.is_ignore_case_set() {
            value.insert("x-case-insensitive".into(), json!(true));
        }
        // `--invoice -` reads the value from standard input (spec 010).
        if arg
            .get_long()
            .is_some_and(|name| STDIN_OPTIONS.contains(&name))
        {
            value.insert("x-stdin".into(), json!(STDIN_VALUE));
        }
    }
    // A repeatable option takes a list: each occurrence is one item.
    let mut property = if is_repeatable(arg) {
        let mut array = Map::new();
        array.insert("type".into(), json!("array"));
        array.insert("items".into(), Value::Object(value));
        if arg.is_required_set() {
            array.insert("minItems".into(), json!(1));
        }
        array
    } else {
        value
    };
    if let Some(help) = arg.get_help() {
        property.insert("description".into(), json!(help.to_string()));
    }
    Value::Object(property)
}

/// Whether the option is a switch without a value, such as `--yes`.
fn is_flag(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse)
}

/// Whether the option collects several values, by repeating it or in one go.
fn is_repeatable(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::Append)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1)
}

/// The global options of the Clap root, each with the canonical names of
/// `entries` it takes effect on: `--output` on every machine command,
/// `--reply-timeout` on those that wait for a reply (a request or a write).
pub(super) fn global_options(root: &Command, entries: &[CommandEntry]) -> Vec<SchemaDocument> {
    let names = |wanted: &dyn Fn(JsonSupport) -> bool| -> Vec<&'static str> {
        let mut names: Vec<_> = entries
            .iter()
            .filter(|entry| wanted(entry.json))
            .map(|entry| entry.canonical)
            .collect();
        names.sort_unstable();
        names
    };
    let mut options: Vec<SchemaDocument> = root
        .get_arguments()
        .filter(|arg| arg.is_global_set() && !arg.is_hide_set())
        .filter_map(|arg| {
            let name = arg.get_long()?;
            let Value::Object(mut option) = property(arg) else {
                return None;
            };
            option.insert("name".into(), json!(name));
            if let Some(default) = arg.get_default_values().first() {
                option.insert("default".into(), json!(default.to_string_lossy()));
            }
            let applies_to = if name == REPLY_TIMEOUT_LONG {
                names(&|json| matches!(json, JsonSupport::Request | JsonSupport::Write))
            } else {
                names(&|json| json != JsonSupport::Unsupported)
            };
            option.insert("applies_to".into(), json!(applies_to));
            Some(option)
        })
        .collect();
    options.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    options
}
