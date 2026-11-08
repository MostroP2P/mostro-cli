//! Offline discovery data: the commands this build knows, which of them speak
//! JSON, and the configuration names they read. Built from the registry and
//! constants only; no environment value, identity or network is consulted.

use clap::{Command, CommandFactory};
use schemars::JsonSchema;
use serde::Serialize;

use super::input_schema::global_options;
pub use super::input_schema::input_schema;
use super::registry::{CommandEntry, JsonSupport, REGISTRY};
use super::Cli;
use crate::commands::order::info::COMMAND as ORDER_INFO;
use crate::commands::order::list::COMMAND as ORDER_LIST;
use crate::commands::order::new::{COMMAND as ORDER_NEW, UNCERTAIN_CODES};
use crate::outcome::config::{config_set_rules, ConfigData, CONFIG_SET_COMMAND};
use crate::outcome::dispute::{DisputeOpenData, COMMAND as DISPUTE_OPEN};
use crate::outcome::identity::{
    IdentityInitData, IdentityShowData, INIT_COMMAND as IDENTITY_INIT, INIT_ERRORS,
    SHOW_COMMAND as IDENTITY_SHOW, SHOW_ERRORS,
};
use crate::outcome::info::OrderInfoData;
use crate::outcome::invoice::{
    BondInvoiceData, OrderInvoiceData, BOND_ERRORS as INVOICE_BOND_ERRORS,
    ORDER_ERRORS as INVOICE_ORDER_ERRORS, ORDER_WARNINGS as INVOICE_ORDER_WARNINGS,
    UNCERTAIN as INVOICE_UNCERTAIN,
};
use crate::outcome::messages::{
    MessageListData, ERRORS as MESSAGE_ERRORS, WARNINGS as MESSAGE_WARNINGS,
};
use crate::outcome::mine::{
    OrderMineData, COMMAND as ORDER_MINE, ERRORS as MINE_ERRORS, WARNINGS as MINE_WARNINGS,
};
use crate::outcome::new::OrderNewData;
use crate::outcome::order::{ListData, Snapshot};
use crate::outcome::rate::{RateData, COMMAND as ORDER_RATE};
use crate::outcome::schema::{output_schema, OutcomeRules, Stages};
use crate::outcome::step::{StepData, STEP_ERRORS, STEP_UNCERTAIN, STEP_WARNINGS};
use crate::outcome::sync::{
    SyncData, ERRORS as SYNC_ERRORS, UNCERTAIN as SYNC_UNCERTAIN, WARNINGS as SYNC_WARNINGS,
};
use crate::outcome::take::{OrderTakeData, TakeSide};
use crate::outcome::{ErrorCode, WarningCode, INVALID_INVOICE};

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
    /// Names of the required settings, each from its root option, its
    /// environment variable or the config-file key of that name; `null` when
    /// not modeled.
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
    /// The options accepted at any command depth, sorted by name: `name`, the
    /// option's schema property (`type`, `enum`, `description`), `default` when
    /// it has one and `applies_to`, the canonical names it takes effect on.
    pub global_options: Vec<SchemaDocument>,
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
        global_options: global_options(&Cli::command(), entries),
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
        "order.list" | ORDER_INFO | ORDER_NEW | ORDER_TAKE_SELL | ORDER_TAKE_BUY
        | ORDER_ADD_INVOICE | BOND_ADD_INVOICE | ORDER_FIAT_SENT | ORDER_RELEASE | ORDER_CANCEL
        | MESSAGE_LIST | ORDER_SYNC | DISPUTE_OPEN | ORDER_RATE => {
            Some(LISTING_CONFIGURATION.to_vec())
        }
        // Local only: the data directory, which has a default, is all they read.
        SCHEMA_COMMAND | IDENTITY_INIT | IDENTITY_SHOW | ORDER_MINE | CONFIG_SET_COMMAND => {
            Some(Vec::new())
        }
        _ => None,
    }
}

/// The discovery document of this build: every registered command with the
/// documents generated from the real Clap grammar and the owned outcome types.
/// The `schema` command, the checked-in fixtures and their drift checks all
/// use this one path.
pub fn offline_discovery() -> DiscoveryData {
    discovery(|entry| command_schemas(&Cli::command(), entry))
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

/// Codes that can fail `order.info` once it is resolved: its own set plus the
/// pilot codes that still apply. `output_not_supported` and the read-only
/// limits belong to other commands.
const INFO_ERRORS: [ErrorCode; 14] = [
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
    ErrorCode::ReplyMalformed,
];

/// Codes that can fail `order.new` with a plain error (nothing sent or a
/// refusal). The ones that leave it `uncertain` are
/// [`UNCERTAIN_CODES`].
const NEW_ERRORS: [ErrorCode; 14] = [
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
    ErrorCode::NodeRefused,
    ErrorCode::OrderDeclined,
    ErrorCode::StoreUnwritable,
];

/// Canonical names of the two takes.
const MESSAGE_LIST: &str = "message.list";
const ORDER_SYNC: &str = "order.sync";
const ORDER_ADD_INVOICE: &str = "order.add-invoice";
const BOND_ADD_INVOICE: &str = "bond.add-invoice";
const ORDER_FIAT_SENT: &str = "order.fiat-sent";
const ORDER_RELEASE: &str = "order.release";
const ORDER_CANCEL: &str = "order.cancel";
const ORDER_TAKE_SELL: &str = "order.take-sell";
const ORDER_TAKE_BUY: &str = "order.take-buy";

/// The reason an `invalid_arguments` of a command that takes an invoice may
/// carry when the invoice value itself was refused.
const INVOICE_REASONS: [&str; 1] = [INVALID_INVOICE];

/// Warnings of `order.new` beyond the pilot pair.
const NEW_WARNINGS: [WarningCode; 1] = [WarningCode::InvalidOrderTimestamp];

/// Warnings of `order.info` beyond the pilot pair.
const INFO_WARNINGS: [WarningCode; 2] = [
    WarningCode::OrdersNotReturned,
    WarningCode::InvalidOrderTimestamp,
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
        ORDER_LIST => output_schema::<ListData, Snapshot>(&OutcomeRules::read(
            ORDER_LIST,
            &LISTING_ERRORS,
            true,
        )),
        ORDER_INFO => output_schema::<OrderInfoData, ()>(&OutcomeRules::request(
            ORDER_INFO,
            &INFO_ERRORS,
            &INFO_WARNINGS,
        )),
        ORDER_NEW => output_schema::<OrderNewData, ()>(&OutcomeRules {
            input_reasons: &INVOICE_REASONS,
            ..OutcomeRules::write(ORDER_NEW, &NEW_ERRORS, &UNCERTAIN_CODES, &NEW_WARNINGS)
        }),
        name @ (ORDER_TAKE_SELL | ORDER_TAKE_BUY) => {
            let side = if name == ORDER_TAKE_SELL {
                TakeSide::Sell
            } else {
                TakeSide::Buy
            };
            // Only take-sell takes an invoice.
            let input_reasons: &[&str] = match side {
                TakeSide::Sell => &INVOICE_REASONS,
                TakeSide::Buy => &[],
            };
            output_schema::<OrderTakeData, ()>(&OutcomeRules {
                input_reasons,
                ..OutcomeRules::write(side.command(), &NEW_ERRORS, &UNCERTAIN_CODES, &NEW_WARNINGS)
            })
        }
        ORDER_ADD_INVOICE => output_schema::<OrderInvoiceData, ()>(&OutcomeRules {
            input_reasons: &INVOICE_REASONS,
            ..OutcomeRules::invoice(
                ORDER_ADD_INVOICE,
                Stages::Update,
                &INVOICE_ORDER_ERRORS,
                &INVOICE_UNCERTAIN,
                &INVOICE_ORDER_WARNINGS,
            )
        }),
        ORDER_FIAT_SENT | ORDER_RELEASE | ORDER_CANCEL => {
            output_schema::<StepData, ()>(&OutcomeRules::step(
                entry.canonical,
                &STEP_ERRORS,
                &STEP_UNCERTAIN,
                &STEP_WARNINGS,
            ))
        }
        BOND_ADD_INVOICE => output_schema::<BondInvoiceData, ()>(&OutcomeRules {
            input_reasons: &INVOICE_REASONS,
            ..OutcomeRules::invoice(
                BOND_ADD_INVOICE,
                Stages::Delivery,
                &INVOICE_BOND_ERRORS,
                &INVOICE_UNCERTAIN,
                &[],
            )
        }),
        MESSAGE_LIST => output_schema::<MessageListData, Snapshot>(&OutcomeRules {
            warnings: &MESSAGE_WARNINGS,
            ..OutcomeRules::read(MESSAGE_LIST, &MESSAGE_ERRORS, true)
        }),
        ORDER_SYNC => output_schema::<SyncData, Snapshot>(&OutcomeRules::sync(
            ORDER_SYNC,
            &SYNC_ERRORS,
            &SYNC_UNCERTAIN,
            &SYNC_WARNINGS,
        )),
        ORDER_MINE => output_schema::<OrderMineData, ()>(&OutcomeRules {
            warnings: &MINE_WARNINGS,
            ..OutcomeRules::read(ORDER_MINE, &MINE_ERRORS, false)
        }),
        DISPUTE_OPEN => output_schema::<DisputeOpenData, ()>(&OutcomeRules::invoice(
            DISPUTE_OPEN,
            Stages::Update,
            &STEP_ERRORS,
            &STEP_UNCERTAIN,
            &STEP_WARNINGS,
        )),
        ORDER_RATE => output_schema::<RateData, ()>(&OutcomeRules::invoice(
            ORDER_RATE,
            Stages::Delivery,
            &STEP_ERRORS,
            &STEP_UNCERTAIN,
            &[],
        )),
        IDENTITY_INIT => {
            output_schema::<IdentityInitData, ()>(&OutcomeRules::local(IDENTITY_INIT, &INIT_ERRORS))
        }
        IDENTITY_SHOW => output_schema::<IdentityShowData, ()>(&OutcomeRules::read(
            IDENTITY_SHOW,
            &SHOW_ERRORS,
            false,
        )),
        CONFIG_SET_COMMAND => output_schema::<ConfigData, ()>(&config_set_rules()),
        SCHEMA_COMMAND => output_schema::<DiscoveryData, ()>(&OutcomeRules::read(
            SCHEMA_COMMAND,
            &DISCOVERY_ERRORS,
            false,
        )),
        _ => return None,
    };
    Some(CommandSchemas {
        input: input_schema(root, entry.path)?,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::outcome::order::{SIDES, STATUSES};
    use clap::CommandFactory;
    use serde_json::{json, Map, Value};

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
    fn the_identity_commands_need_no_node_configuration() {
        let data = discovery(schemas);
        for name in ["identity.init", "identity.show"] {
            let command = by_name(&data, name);
            assert_eq!(command.required_configuration, Some(vec![]), "{name}");
            assert_eq!(
                command.output_modes,
                [PresentationMode::Human, PresentationMode::Json]
            );
        }
        let input = input_schema(&Cli::command(), &["identity", "init"]).unwrap();
        assert_eq!(input["properties"], json!({}));
        assert_eq!(input["required"], json!([]));
    }

    /// `--invoice -` (spec 010): exactly the four commands that take an
    /// invoice say that `-` reads it from standard input; no other option does.
    #[test]
    fn the_invoice_option_is_marked_as_readable_from_stdin() {
        let root = Cli::command();
        let mut marked = Vec::new();
        for entry in REGISTRY
            .iter()
            .filter(|e| e.json != JsonSupport::Unsupported)
        {
            let input = input_schema(&root, entry.path).unwrap();
            for (name, property) in input["properties"].as_object().unwrap() {
                if let Some(value) = property.get("x-stdin") {
                    assert_eq!(value, "-", "{}", entry.canonical);
                    let help = property["description"].as_str().unwrap();
                    assert!(help.contains("`-` reads it from standard input"), "{help}");
                    marked.push((entry.canonical, name.clone()));
                }
            }
        }
        marked.sort();
        let invoice = |command| (command, "invoice".to_string());
        assert_eq!(
            marked,
            [
                invoice("bond.add-invoice"),
                invoice("order.add-invoice"),
                invoice("order.new"),
                invoice("order.take-sell"),
            ]
        );
    }

    /// `order new --expires-in` (spec 012) is discoverable, and optional.
    #[test]
    fn order_new_lists_expires_in() {
        let input = input_schema(&Cli::command(), &["order", "new"]).unwrap();
        let expires = &input["properties"]["expires-in"];
        assert_eq!(expires["type"], "string");
        assert!(expires["description"]
            .as_str()
            .unwrap()
            .contains("31536000"));
        assert!(!strings(&input["required"]).contains(&"expires-in"));
    }

    /// `--reply-timeout` is discoverable: its default and range, and exactly
    /// the commands that wait for the node's reply; `--output` is listed too.
    #[test]
    fn the_global_options_are_listed_with_what_they_apply_to() {
        let data = discovery(schemas);
        let names: Vec<&str> = data
            .global_options
            .iter()
            .map(|option| option["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["output", "reply-timeout"]);
        let reply = &data.global_options[1];
        assert_eq!(reply["type"], "string");
        assert_eq!(reply["default"], "15");
        assert!(reply["description"].as_str().unwrap().contains("5 to 300"));
        let waiting: Vec<&str> = {
            let mut names: Vec<&str> = REGISTRY
                .iter()
                .filter(|entry| matches!(entry.json, JsonSupport::Request | JsonSupport::Write))
                .map(|entry| entry.canonical)
                .collect();
            names.sort_unstable();
            names
        };
        assert_eq!(reply["applies_to"], json!(waiting));
        assert_eq!(waiting.len(), 11);
        for read in ["order.list", "message.list", "order.sync", "schema"] {
            assert!(!waiting.contains(&read), "{read}");
        }
        assert_eq!(data.global_options[0]["enum"], json!(["human", "json"]));
        let input = input_schema(&Cli::command(), &["order", "take-sell"]).unwrap();
        assert!(
            input["properties"].get("reply-timeout").is_none(),
            "not command input"
        );
    }

    #[test]
    fn serialization_keeps_every_key_and_null() {
        let value = serde_json::to_value(discovery(schemas)).unwrap();
        let legacy = value["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == "dispute.list")
            .unwrap();
        assert_eq!(
            legacy,
            &json!({
                "name": "dispute.list",
                "cli_path": ["listdisputes"],
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
    fn a_flag_is_a_boolean() {
        let input = input_schema(&Cli::command(), &["order", "new"]).unwrap();
        let yes = &input["properties"]["yes"];
        assert_eq!(yes["type"], "boolean");
        assert!(yes.get("enum").is_none(), "{yes}");
    }

    #[test]
    fn a_repeatable_option_is_an_array_of_its_values() {
        let input = input_schema(&Cli::command(), &["order", "info"]).unwrap();
        let order = &input["properties"]["order"];
        assert_eq!(order["type"], "array");
        assert_eq!(order["items"], json!({"type": "string"}));
        assert_eq!(order["minItems"], 1);
        assert_eq!(input["required"], json!(["order"]));

        let list = input_schema(&Cli::command(), &["order", "list"]).unwrap();
        assert_eq!(list["properties"]["side"]["type"], "string");
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
        let input = input_schema(&Cli::command(), &["order", "new"]).unwrap();
        let required = strings(&input["required"]);
        for name in ["side", "currency", "fiat-amount", "payment-method"] {
            assert!(required.contains(&name), "{name}");
        }
        for name in ["premium", "sats", "fiat-amount-max", "invoice", "yes"] {
            assert!(!required.contains(&name), "{name}");
        }
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
        let legacy = by_name(&data, "dispute.list");
        assert_eq!(
            (&legacy.input_schema, &legacy.output_schema),
            (&None, &None)
        );
    }
}
