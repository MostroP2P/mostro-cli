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
pub use commands::Commands;
pub use context::Context;

use std::{env, process};

use anyhow::{anyhow, Result};
use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};

use crate::outcome::ARGUMENT_ERROR_EXIT;
use crate::util::net::install_rustls_crypto_provider;
use context::init_context;
use invocation::{parse_invocation, Invocation, OutputMode, OutputSelection};

const JSON_NOT_SUPPORTED: &str = "error: JSON output is not supported for this command\n";

pub async fn run() -> Result<()> {
    let cli = match parse_invocation(Cli::command(), env::args_os()) {
        Invocation::Parsed {
            matches,
            output: OutputMode::Human,
        } => Cli::from_arg_matches(&matches)
            .map_err(|_| anyhow!("internal error: parsed arguments could not be bound"))?,
        Invocation::Parsed {
            output: OutputMode::Json,
            ..
        } => reject(JSON_NOT_SUPPORTED),
        Invocation::Text(display) => display.exit(),
        Invocation::Rejected(failure)
            if failure.kind == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                && failure.output == OutputSelection::Mode(OutputMode::Human) =>
        {
            reject(&Cli::command().render_help().to_string())
        }
        Invocation::Rejected(failure) => reject(&failure.diagnostic()),
    };
    install_rustls_crypto_provider();

    // Daemon-local gRPC commands: no relays, keys or database involved.
    if let Some(cmd) = &cli.command {
        if let Some(result) = cmd.run_rpc().await {
            return result;
        }
    }

    let ctx = init_context(&cli).await?;

    if let Some(cmd) = &cli.command {
        cmd.run(&ctx).await?;
    }

    Ok(())
}

/// Report an argument error on stderr and exit before any effect.
fn reject(diagnostic: &str) -> ! {
    eprint!("{diagnostic}");
    process::exit(ARGUMENT_ERROR_EXIT)
}
