//! Offline discovery data: the commands this build knows, which of them speak
//! JSON, and the configuration names they read. Built from the registry and
//! constants only; no environment value, identity or network is consulted.

use serde::Serialize;
use serde_json::{Map, Value};

use super::registry::{CommandEntry, JsonSupport, REGISTRY};

/// The contract version of discovery and outcome documents, independent of the
/// package version.
pub const SCHEMA_VERSION: &str = "1";

/// The JSON Schema dialect of every generated schema document.
pub const SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Environment variable names the public listing reads; the root options
/// `--mostro-pubkey` and `--relays` satisfy them.
const LISTING_CONFIGURATION: [&str; 2] = ["MOSTRO_PUBKEY", "RELAYS"];

/// A generated JSON Schema document. Schemas are plain objects so discovery
/// never needs a schema of a schema.
pub type SchemaDocument = Map<String, Value>;

/// Input and output schemas of one machine-capable command.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSchemas {
    pub input: SchemaDocument,
    pub output: SchemaDocument,
}

/// How a command can present its outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentationMode {
    Human,
    Json,
}

/// One discoverable command.
#[derive(Clone, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize)]
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
        "schema" => Some(Vec::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
