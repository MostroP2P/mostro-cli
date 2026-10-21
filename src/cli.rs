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

use anyhow::Result;
use clap::Parser;

use crate::util::net::install_rustls_crypto_provider;
use context::init_context;

pub async fn run() -> Result<()> {
    install_rustls_crypto_provider();
    let cli = Cli::parse();

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
