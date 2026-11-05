pub mod add_bond_invoice;
pub mod add_invoice;
pub mod adm_send_dm;
pub mod conversation_key;
pub mod dispute_chat;
pub mod dm_to_user;
pub mod get_dm;
pub mod get_dm_user;
pub mod invocation;
pub mod last_trade_index;
pub mod list_disputes;
pub mod maintenance;
pub mod rate_user;
pub mod restore;
pub mod send_admin_dm_attach;
pub mod send_dm;
pub mod send_msg;
pub mod take_dispute;
pub mod take_order;
pub mod user_info;

mod args;
mod commands;
mod context;
mod legacy;
mod migrated;
mod order_new;
mod registry;
mod schema;

pub use args::Cli;
pub use commands::{Commands, NewArgs, OrderCommands};
pub use context::Context;
pub use registry::{lookup, CommandEntry, JsonSupport, REGISTRY};
pub use schema::{
    command_schemas, discovery, input_schema, offline_discovery, CommandDescriptor, CommandSchemas,
    DiscoveryData, PresentationMode, SchemaDocument, SCHEMA_DIALECT, SCHEMA_VERSION,
};

use std::env;
use std::io::{self, Write};

use anyhow::Result;
use clap::error::ErrorKind;
use clap::{ArgMatches, CommandFactory, FromArgMatches};

use crate::outcome::{
    Envelope, ErrorCode, ExecutionMeta, Warnings, ARGUMENT_ERROR_EXIT, RUNTIME_ERROR_EXIT,
};
use crate::parse_machine::emit;
use crate::util::net::install_rustls_crypto_provider;
use context::init_context;
use invocation::{parse_invocation, ArgumentFailure, Invocation, OutputMode, OutputSelection};
use migrated::diagnose;

/// Parse argv, run the command and return the process exit status.
pub async fn run() -> Result<i32> {
    let (matches, output) = match parse_invocation(Cli::command(), env::args_os()) {
        Invocation::Parsed { matches, output } => (matches, output),
        Invocation::Text(display) => {
            let _ = display.print();
            return Ok(display.exit_code());
        }
        Invocation::Rejected(failure) => return Ok(report(&failure)),
    };
    let Ok(cli) = Cli::from_arg_matches(&matches) else {
        return Ok(unbound(
            &matches,
            output,
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        ));
    };

    if let Some(exit) = migrated::run(&cli, output).await {
        return Ok(exit);
    }
    if output == OutputMode::Json {
        return Ok(reject_json(&matches));
    }
    install_rustls_crypto_provider();

    // Daemon-local gRPC commands: no relays, keys or database involved.
    if let Some(cmd) = &cli.command {
        if let Some(result) = cmd.run_rpc().await {
            return result.map(|()| 0);
        }
    }

    let ctx = init_context(&cli).await?;

    if let Some(cmd) = &cli.command {
        cmd.run(&ctx).await?;
    }

    Ok(0)
}

/// The subcommand names from the root to the selected leaf.
fn command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut level = matches;
    while let Some((name, sub)) = level.subcommand() {
        path.push(name.to_string());
        level = sub;
    }
    path
}

/// JSON for an accepted invocation of a command without JSON support. This
/// answers before RPC routing, environment access or context initialization.
/// Root options without a command name no command and are invalid.
fn reject_json(matches: &ArgMatches) -> i32 {
    let path = command_path(matches);
    if path.is_empty() {
        rejection(None, ErrorCode::InvalidArguments)
    } else {
        rejection(lookup(&path), ErrorCode::OutputNotSupported)
    }
}

/// An argument error: a JSON envelope when JSON was clearly selected,
/// otherwise fixed text on stderr.
fn report(failure: &ArgumentFailure) -> i32 {
    if let Some(code) = failure.json_error_code() {
        let entry = failure.command.as_deref().and_then(lookup);
        return rejection(entry, code);
    }
    let bare = failure.kind == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        && failure.output == OutputSelection::Mode(OutputMode::Human);
    let text = if bare {
        Cli::command().render_help().to_string()
    } else {
        failure.diagnostic().to_string()
    };
    let _ = io::stderr().lock().write_all(text.as_bytes());
    ARGUMENT_ERROR_EXIT
}

fn rejection(entry: Option<&CommandEntry>, code: ErrorCode) -> i32 {
    reject_to(
        entry,
        code,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}

fn reject_to(
    entry: Option<&CommandEntry>,
    code: ErrorCode,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    let stages = entry.map_or_else(ExecutionMeta::not_executed, CommandEntry::stages);
    let outcome = Envelope::<(), ()>::error(
        entry.map(|entry| entry.canonical),
        code,
        Warnings::new(),
        stages,
    );
    emit(&outcome, out, err)
}

/// Parsed arguments that cannot be bound to the typed grammar: an internal
/// error, reported in the selected mode with the resolved command identity.
fn unbound(
    matches: &ArgMatches,
    output: OutputMode,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    match output {
        OutputMode::Json => reject_to(
            lookup(&command_path(matches)),
            ErrorCode::InternalError,
            out,
            err,
        ),
        OutputMode::Human => {
            diagnose(
                err,
                &format!("error: {}", ErrorCode::InternalError.message()),
            );
            RUNTIME_ERROR_EXIT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An accepted invocation that cannot be bound to the typed arguments is
    /// an internal error: one JSON document when JSON was selected, fixed text
    /// on stderr and empty stdout otherwise; always exit 1.
    #[test]
    fn unbindable_arguments_are_an_internal_error_in_the_selected_mode() {
        let matches = |args: &[&str]| Cli::command().get_matches_from(args);
        let listing = matches(&["mostro-cli", "order", "list"]);
        let legacy = matches(&["mostro-cli", "getdm"]);

        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(unbound(&listing, OutputMode::Json, &mut out, &mut err), 1);
        let document: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(document["command"], "order.list");
        assert_eq!(document["status"], "error");
        assert_eq!(document["data"], serde_json::Value::Null);
        assert_eq!(document["error"]["code"], "internal_error");
        assert_eq!(document["meta"]["publication"], "not_applicable");
        assert!(err.is_empty());

        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(unbound(&legacy, OutputMode::Json, &mut out, &mut err), 1);
        let document: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(document["command"], "message.list");
        assert_eq!(document["meta"]["publication"], "not_attempted");

        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(unbound(&legacy, OutputMode::Human, &mut out, &mut err), 1);
        assert!(out.is_empty());
        assert_eq!(
            String::from_utf8(err).unwrap(),
            format!("error: {}\n", ErrorCode::InternalError.message())
        );
    }
}
