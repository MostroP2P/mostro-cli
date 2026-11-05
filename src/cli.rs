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
pub mod list_orders;
pub mod maintenance;
pub mod new_order;
pub mod orders_info;
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

pub use args::Cli;
pub use commands::{lookup, CommandEntry, Commands, JsonSupport, REGISTRY};
pub use context::Context;

use std::{env, io};

use anyhow::{anyhow, Result};
use clap::error::ErrorKind;
use clap::{ArgMatches, CommandFactory, FromArgMatches};

use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings, ARGUMENT_ERROR_EXIT};
use crate::parse_machine::emit;
use crate::util::net::install_rustls_crypto_provider;
use context::init_context;
use invocation::{parse_invocation, ArgumentFailure, Invocation, OutputMode, OutputSelection};

/// Parse argv, run the command and return the process exit status.
pub async fn run() -> Result<i32> {
    let cli = match parse_invocation(Cli::command(), env::args_os()) {
        Invocation::Parsed {
            matches,
            output: OutputMode::Human,
        } => Cli::from_arg_matches(&matches)
            .map_err(|_| anyhow!("internal error: parsed arguments could not be bound"))?,
        Invocation::Parsed {
            matches,
            output: OutputMode::Json,
        } => return Ok(reject_json(&matches)),
        Invocation::Text(display) => {
            let _ = display.print();
            return Ok(display.exit_code());
        }
        Invocation::Rejected(failure) => return Ok(report(&failure)),
    };
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

/// JSON for an invocation Clap accepted. No command supports it yet, so this
/// answers before RPC routing, environment access or context initialization.
/// Root options without a command name no command and are invalid.
fn reject_json(matches: &ArgMatches) -> i32 {
    match matches.subcommand_name() {
        Some(name) => rejection(lookup(&[name.to_string()]), ErrorCode::OutputNotSupported),
        None => rejection(None, ErrorCode::InvalidArguments),
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
    if bare {
        eprint!("{}", Cli::command().render_help());
    } else {
        eprint!("{}", failure.diagnostic());
    }
    ARGUMENT_ERROR_EXIT
}

fn rejection(entry: Option<&CommandEntry>, code: ErrorCode) -> i32 {
    let stages = entry.map_or_else(ExecutionMeta::not_executed, CommandEntry::stages);
    let outcome = Envelope::<(), ()>::error(
        entry.map(|entry| entry.canonical),
        code,
        Warnings::new(),
        stages,
    );
    emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
}
