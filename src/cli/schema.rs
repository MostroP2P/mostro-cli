//! Offline discovery data: the commands this build knows, which of them speak
//! JSON, and the configuration names they read. Built from the registry and
//! constants only; no environment value, identity or network is consulted.

use clap::{Arg, Command};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::invocation::OUTPUT_LONG;
use super::registry::{CommandEntry, JsonSupport, REGISTRY};
use crate::commands::order::list::COMMAND as ORDER_LIST;
use crate::outcome::order::{ListData, ListOrdersInput, Snapshot};
use crate::outcome::schema::{output_schema, OutcomeRules};
use crate::outcome::ErrorCode;

pub use crate::outcome::schema::{SchemaDocument, DIALECT as SCHEMA_DIALECT};
pub use crate::outcome::SCHEMA_VERSION;

/// Environment variable names the public listing reads; the root options
/// `--mostro-pubkey` and `--relays` satisfy them.
const LISTING_CONFIGURATION: [&str; 2] = ["MOSTRO_PUBKEY", "RELAYS"];

/// Input and output schemas of one machine-capable command.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSchemas {
    pub input: SchemaDocument,
    pub output: SchemaDocument,
}

/// How a command can present its outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PresentationMode {
    Human,
    Json,
}

/// One discoverable command.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct CommandDescriptor {
    /// Canonical dotted name, as reported in outcomes.
    pub name: &'static str,
    /// The invocation tokens as typed today.
    pub cli_path: Vec<&'static str>,
    pub output_modes: Vec<PresentationMode>,
    /// `null` for commands without machine output.
    pub input_schema: Option<SchemaDocument>,
    /// `null` for commands without machine output.
    pub output_schema: Option<SchemaDocument>,
    /// Environment variable names read; `null` when not modeled.
    pub required_configuration: Option<Vec<&'static str>>,
}

/// The discovery document: `schema` data in a JSON outcome and, in human mode,
/// printed as pretty JSON.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct DiscoveryData {
    pub schema_version: &'static str,
    pub executable_version: &'static str,
    pub schema_dialect: &'static str,
    pub commands: Vec<CommandDescriptor>,
}

/// Discovery for every registered command. `schemas` supplies the generated
/// documents of the machine-capable commands.
pub fn discovery(schemas: impl Fn(&CommandEntry) -> Option<CommandSchemas>) -> DiscoveryData {
    describe(REGISTRY, schemas)
}

fn describe(
    entries: &[CommandEntry],
    schemas: impl Fn(&CommandEntry) -> Option<CommandSchemas>,
) -> DiscoveryData {
    let mut commands: Vec<CommandDescriptor> = entries
        .iter()
        .map(|entry| descriptor(entry, schemas(entry)))
        .collect();
    commands.sort_by_key(|command| command.name);
    DiscoveryData {
        schema_version: SCHEMA_VERSION,
        executable_version: env!("CARGO_PKG_VERSION"),
        schema_dialect: SCHEMA_DIALECT,
        commands,
    }
}

fn descriptor(entry: &CommandEntry, schemas: Option<CommandSchemas>) -> CommandDescriptor {
    let machine = entry.json != JsonSupport::Unsupported;
    let (input_schema, output_schema) = match schemas.filter(|_| machine) {
        Some(schemas) => (Some(schemas.input), Some(schemas.output)),
        None => (None, None),
    };
    CommandDescriptor {
        name: entry.canonical,
        cli_path: entry.path.to_vec(),
        output_modes: if machine {
            vec![PresentationMode::Human, PresentationMode::Json]
        } else {
            vec![PresentationMode::Human]
        },
        input_schema,
        output_schema,
        required_configuration: required_configuration(entry),
    }
}

/// Configuration names of a machine command. Legacy requirements are not
/// modeled, so they are `None` rather than an empty list.
fn required_configuration(entry: &CommandEntry) -> Option<Vec<&'static str>> {
    match entry.canonical {
        "order.list" => Some(LISTING_CONFIGURATION.to_vec()),
        SCHEMA_COMMAND => Some(Vec::new()),
        _ => None,
    }
}

/// Codes that can fail `order.list` once it is resolved. `output_not_supported`
/// belongs to other commands.
const LISTING_ERRORS: [ErrorCode; 8] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::ReadTimeout,
    ErrorCode::ReadLimitExceeded,
    ErrorCode::InternalError,
];

/// Canonical name of offline discovery.
pub const SCHEMA_COMMAND: &str = "schema";

/// Codes that can fail `schema` once it is resolved: it reads no configuration
/// and touches nothing, so only an internal failure remains.
const DISCOVERY_ERRORS: [ErrorCode; 2] = [ErrorCode::InvalidArguments, ErrorCode::InternalError];

/// Generated documents of a machine-capable command, or `None` for one this
/// build cannot describe yet. `root` is the real Clap grammar.
pub fn command_schemas(root: &Command, entry: &CommandEntry) -> Option<CommandSchemas> {
    let output = match entry.canonical {
        ORDER_LIST => output_schema::<ListData, Snapshot>(&OutcomeRules {
            command: ORDER_LIST,
            errors: &LISTING_ERRORS,
            snapshot_on_success: true,
        }),
        SCHEMA_COMMAND => output_schema::<DiscoveryData, ()>(&OutcomeRules {
            command: SCHEMA_COMMAND,
            errors: &DISCOVERY_ERRORS,
            snapshot_on_success: false,
        }),
        _ => return None,
    };
    Some(CommandSchemas {
        input: input_schema(root, entry.path)?,
        output,
    })
}

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

fn is_input(arg: &Arg) -> bool {
    !arg.is_hide_set()
        && !matches!(arg.get_id().as_str(), "help" | "version" | OUTPUT_LONG)
        && arg.get_long().is_some()
}

fn property(arg: &Arg) -> Value {
    let mut property = Map::new();
    property.insert("type".into(), json!("string"));
    if let Some(help) = arg.get_help() {
        property.insert("description".into(), json!(help.to_string()));
    }
    let values: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect();
    if !values.is_empty() {
        property.insert("enum".into(), json!(values));
    }
    if arg.is_ignore_case_set() {
        property.insert("x-case-insensitive".into(), json!(true));
    }
    Value::Object(property)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::outcome::order::{SIDES, STATUSES};
    use clap::CommandFactory;
    use serde_json::json;

    fn schemas(_: &CommandEntry) -> Option<CommandSchemas> {
        Some(CommandSchemas {
            input: Map::from_iter([("type".to_string(), json!("object"))]),
            output: Map::from_iter([("type".to_string(), json!("object"))]),
        })
    }

    fn by_name<'a>(data: &'a DiscoveryData, name: &str) -> &'a CommandDescriptor {
        data.commands
            .iter()
            .find(|command| command.name == name)
            .unwrap_or_else(|| panic!("{name} is not described"))
    }

    #[test]
    fn header_names_the_contract_and_the_build() {
        let data = discovery(schemas);
        assert_eq!(data.schema_version, "1");
        assert_eq!(data.executable_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            data.schema_dialect,
            "https://json-schema.org/draft/2020-12/schema"
        );
    }

    #[test]
    fn every_registered_command_is_described_once_sorted_by_name() {
        let data = discovery(schemas);
        let names: Vec<&str> = data.commands.iter().map(|command| command.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(names.len(), REGISTRY.len());
        for entry in REGISTRY {
            let command = by_name(&data, entry.canonical);
            assert_eq!(command.cli_path, entry.path, "{}", entry.canonical);
        }
    }

    #[test]
    fn legacy_commands_are_human_only_with_null_schemas_and_configuration() {
        let data = discovery(schemas);
        for entry in REGISTRY
            .iter()
            .filter(|entry| entry.json == JsonSupport::Unsupported)
        {
            let command = by_name(&data, entry.canonical);
            assert_eq!(command.output_modes, [PresentationMode::Human]);
            assert_eq!(command.input_schema, None, "{}", entry.canonical);
            assert_eq!(command.output_schema, None, "{}", entry.canonical);
            assert_eq!(command.required_configuration, None, "{}", entry.canonical);
        }
    }

    #[test]
    fn the_listing_advertises_json_its_schemas_and_configuration_names() {
        let data = discovery(schemas);
        let listing = by_name(&data, "order.list");
        assert_eq!(listing.cli_path, ["order", "list"]);
        assert_eq!(
            listing.output_modes,
            [PresentationMode::Human, PresentationMode::Json]
        );
        assert!(listing.input_schema.is_some() && listing.output_schema.is_some());
        assert_eq!(
            listing.required_configuration,
            Some(vec!["MOSTRO_PUBKEY", "RELAYS"])
        );
    }

    #[test]
    fn a_machine_command_without_supplied_schemas_has_none() {
        let data = discovery(|_| None);
        let listing = by_name(&data, "order.list");
        assert_eq!(
            (&listing.input_schema, &listing.output_schema),
            (&None, &None)
        );
    }

    #[test]
    fn schema_requires_no_configuration() {
        let entries = [
            CommandEntry {
                path: &["schema"],
                canonical: "schema",
                json: JsonSupport::Read,
            },
            REGISTRY[0],
        ];
        let data = describe(&entries, schemas);
        let names: Vec<&str> = data.commands.iter().map(|command| command.name).collect();
        assert_eq!(names, ["order.list", "schema"]);
        let schema = by_name(&data, "schema");
        assert_eq!(schema.cli_path, ["schema"]);
        assert_eq!(schema.required_configuration, Some(vec![]));
    }

    #[test]
    fn serialization_keeps_every_key_and_null() {
        let value = serde_json::to_value(discovery(schemas)).unwrap();
        let legacy = value["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == "order.new")
            .unwrap();
        assert_eq!(
            legacy,
            &json!({
                "name": "order.new",
                "cli_path": ["neworder"],
                "output_modes": ["human"],
                "input_schema": null,
                "output_schema": null,
                "required_configuration": null,
            })
        );
        assert_eq!(value["commands"][0]["name"], "account.conversation-key");
    }

    fn strings(value: &Value) -> Vec<&str> {
        value
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    #[test]
    fn the_listing_input_comes_from_clap_and_the_shared_validators() {
        let input = input_schema(&Cli::command(), &["order", "list"]).unwrap();
        assert_eq!(input["$schema"], SCHEMA_DIALECT);
        assert_eq!(input["type"], "object");
        assert_eq!(input["additionalProperties"], json!(false));
        assert_eq!(input["required"], json!([]));
        let properties = input["properties"].as_object().unwrap();
        let names: Vec<&str> = properties.keys().map(String::as_str).collect();
        assert_eq!(names, ["currency", "side", "status"]);
        assert_eq!(strings(&properties["side"]["enum"]), SIDES);
        assert_eq!(strings(&properties["status"]["enum"]), STATUSES);
        assert_eq!(properties["status"]["default"], "pending");
        assert_eq!(properties["side"]["x-case-insensitive"], true);
        assert_eq!(properties["status"]["x-case-insensitive"], true);
        assert_eq!(properties["currency"]["type"], "string");
        assert!(properties["currency"].get("enum").is_none());
        assert!(properties["side"]["description"]
            .as_str()
            .unwrap()
            .contains("buy or sell"));
        // Output selection and help are global, not command input.
        for hidden in ["output", "help", "version"] {
            assert!(!properties.contains_key(hidden), "{hidden}");
        }
    }

    #[test]
    fn required_arguments_follow_clap() {
        let input = input_schema(&Cli::command(), &["neworder"]).unwrap();
        let required = strings(&input["required"]);
        for name in ["kind", "fiatcode", "fiatamount", "paymentmethod"] {
            assert!(required.contains(&name), "{name}");
        }
        assert!(!required.contains(&"premium"));
        assert_eq!(input_schema(&Cli::command(), &["nosuch"]), None);
        assert_eq!(input_schema(&Cli::command(), &["order", "nosuch"]), None);
    }

    #[test]
    fn every_machine_command_has_generated_schemas_and_legacy_has_none() {
        let root = Cli::command();
        for entry in REGISTRY {
            let generated = command_schemas(&root, entry);
            if entry.json == JsonSupport::Unsupported {
                assert!(generated.is_none(), "{}", entry.canonical);
            } else {
                let generated = generated.unwrap_or_else(|| panic!("{}", entry.canonical));
                assert_eq!(generated.output["$schema"], SCHEMA_DIALECT);
                assert_eq!(generated.input["$schema"], SCHEMA_DIALECT);
            }
        }
    }

    #[test]
    fn discovery_carries_the_generated_listing_schemas() {
        let root = Cli::command();
        let data = discovery(|entry| command_schemas(&root, entry));
        let listing = by_name(&data, "order.list");
        let output = listing.output_schema.as_ref().unwrap();
        assert_eq!(output["title"], "order.list outcome");
        assert!(output.contains_key("$defs") && output.contains_key("oneOf"));
        assert!(listing.input_schema.is_some());
        let legacy = by_name(&data, "order.new");
        assert_eq!(
            (&legacy.input_schema, &legacy.output_schema),
            (&None, &None)
        );
    }
}
