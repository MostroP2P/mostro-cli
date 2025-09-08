pub mod add_invoice;
pub mod adm_send_dm;
pub mod conversation_key;
pub mod dm_to_user;
pub mod get_dm;
pub mod get_dm_user;
pub mod list_disputes;
pub mod list_orders;
pub mod new_order;
pub mod rate_user;
pub mod restore;
pub mod send_dm;
pub mod send_msg;
pub mod take_buy;
pub mod take_dispute;
pub mod take_sell;

use crate::cli::add_invoice::execute_add_invoice;
use crate::cli::adm_send_dm::execute_adm_send_dm;
use crate::cli::conversation_key::execute_conversation_key;
use crate::cli::dm_to_user::execute_dm_to_user;
use crate::cli::get_dm::execute_get_dm;
use crate::cli::get_dm_user::execute_get_dm_user;
use crate::cli::list_disputes::execute_list_disputes;
use crate::cli::list_orders::execute_list_orders;
use crate::cli::new_order::execute_new_order;
use crate::cli::rate_user::execute_rate_user;
use crate::cli::restore::execute_restore;
use crate::cli::send_dm::execute_send_dm;
use crate::cli::take_buy::execute_take_buy;
use crate::cli::take_dispute::execute_take_dispute;
use crate::cli::take_sell::execute_take_sell;
use crate::db::{connect, User};
use crate::util;

use anyhow::{Error, Result};
use clap::{Parser, Subcommand};
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::sync::OnceLock;
use std::{
    env::{set_var, var},
    str::FromStr,
};
use take_dispute::*;
use uuid::Uuid;

pub static IDENTITY_KEYS: OnceLock<Keys> = OnceLock::new();
pub static MOSTRO_KEYS: OnceLock<Keys> = OnceLock::new();
pub static MOSTRO_PUBKEY: OnceLock<PublicKey> = OnceLock::new();
pub static POOL: OnceLock<SqlitePool> = OnceLock::new();
pub static TRADE_KEY: OnceLock<(Keys, i64)> = OnceLock::new();

pub struct Context {
    pub client: Client,
    pub identity_keys: Keys,
    pub trade_keys: Keys,
    pub trade_index: i64,
    pub pool: SqlitePool,
    pub mostro_keys: Keys,
    pub mostro_pubkey: PublicKey,
}

#[derive(Parser)]
#[command(
    name = "mostro-cli",
    about = "A simple CLI to use Mostro P2P",
    author,
    help_template = "\
{before-help}{name} 🧌

{about-with-newline}
{author-with-newline}
{usage-heading} {usage}

{all-args}{after-help}
",
    version
)]
#[command(propagate_version = true)]
#[command(arg_required_else_help(true))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub mostropubkey: Option<String>,
    #[arg(short, long)]
    pub relays: Option<String>,
    #[arg(short, long)]
    pub pow: Option<String>,
    #[arg(short, long)]
    pub secret: bool,
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum Commands {
    /// Requests open orders from Mostro pubkey
    ListOrders {
        /// Status of the order
        #[arg(short, long)]
        status: Option<String>,
        /// Currency selected
        #[arg(short, long)]
        currency: Option<String>,
        /// Choose an order kind
        #[arg(short, long)]
        kind: Option<String>,
    },
    /// Create a new buy/sell order on Mostro
    NewOrder {
        /// Choose an order kind
        #[arg(short, long)]
        kind: String,
        /// Sats amount - leave empty for market price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        amount: i64,
        /// Currency selected
        #[arg(short = 'c', long)]
        fiat_code: String,
        /// Fiat amount
        #[arg(short, long)]
        #[clap(value_parser=check_fiat_range)]
        fiat_amount: (i64, Option<i64>),
        /// Payment method
        #[arg(short = 'm', long)]
        payment_method: String,
        /// Premium on price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        #[clap(allow_hyphen_values = true)]
        premium: i64,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
        /// Expiration time of a pending Order, in days
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        expiration_days: i64,
    },
    /// Take a sell order from a Mostro pubkey
    TakeSell {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
        /// Amount of fiat to buy
        #[arg(short, long)]
        amount: Option<u32>,
    },
    /// Take a buy order from a Mostro pubkey
    TakeBuy {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Amount of fiat to sell
        #[arg(short, long)]
        amount: Option<u32>,
    },
    /// Buyer add a new invoice to receive the payment
    AddInvoice {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
    /// Get the latest direct messages
    GetDm {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
        /// If true, get messages from counterparty, otherwise from Mostro
        #[arg(short)]
        from_user: bool,
    },
    /// Get direct messages sent to any trade keys
    GetDmUser {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
    },
    /// Get the latest direct messages for admin
    GetAdminDm {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
        /// If true, get messages from counterparty, otherwise from Mostro
        #[arg(short)]
        from_user: bool,
    },
    /// Send direct message to a user
    SendDm {
        /// Pubkey of the counterpart
        #[arg(short, long)]
        pubkey: String,
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Message to send
        #[arg(short, long)]
        message: String,
    },
    /// Send gift wrapped direct message to a user
    DmToUser {
        /// Pubkey of the recipient
        #[arg(short, long)]
        pubkey: String,
        /// Order id to get ephemeral keys
        #[arg(short, long)]
        order_id: Uuid,
        /// Message to send
        #[arg(short, long)]
        message: String,
    },
    /// Send fiat sent message to confirm payment to other user
    FiatSent {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Settle the hold invoice and pay to buyer.
    Release {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Cancel a pending order
    Cancel {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Rate counterpart after a successful trade
    Rate {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Rating from 1 to 5
        #[arg(short, long)]
        rating: u8,
    },
    /// Restore session to recover all pending orders and disputes
    Restore {},
    /// Start a dispute
    Dispute {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Cancel an order (only admin)
    AdmCancel {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Settle a seller's hold invoice (only admin)
    AdmSettle {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Requests open disputes from Mostro pubkey
    AdmListDisputes {},
    /// Add a new dispute's solver (only admin)
    AdmAddSolver {
        /// npubkey
        #[arg(short, long)]
        npubkey: String,
    },
    /// Admin or solver take a Pending dispute (only admin)
    AdmTakeDispute {
        /// Dispute id
        #[arg(short, long)]
        dispute_id: Uuid,
    },
    /// Send gift wrapped direct message to a user (only admin)
    AdmSendDm {
        /// Pubkey of the recipient
        #[arg(short, long)]
        pubkey: String,
        /// Message to send
        #[arg(short, long)]
        message: String,
    },
    /// Get the conversation key for direct messaging with a user
    ConversationKey {
        /// Pubkey of the counterpart
        #[arg(short, long)]
        pubkey: String,
    },
}

fn get_env_var(cli: &Cli) {
    // Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
        pretty_env_logger::init();
    }

    if cli.mostropubkey.is_some() {
        set_var("MOSTRO_PUBKEY", cli.mostropubkey.clone().unwrap());
    }
    let _pubkey = var("MOSTRO_PUBKEY").expect("$MOSTRO_PUBKEY env var needs to be set");

    if cli.relays.is_some() {
        set_var("RELAYS", cli.relays.clone().unwrap());
    }

    if cli.pow.is_some() {
        set_var("POW", cli.pow.clone().unwrap());
    }

    if cli.secret {
        set_var("SECRET", "true");
    }
}

// Check range with two values value
fn check_fiat_range(s: &str) -> Result<(i64, Option<i64>)> {
    if s.contains('-') {
        let min: i64;
        let max: i64;

        // Get values from CLI
        let values: Vec<&str> = s.split('-').collect();

        // Check if more than two values
        if values.len() > 2 {
            return Err(Error::msg("Wrong amount syntax"));
        };

        // Get ranged command
        if let Err(e) = values[0].parse::<i64>() {
            return Err(e.into());
        } else {
            min = values[0].parse().unwrap();
        }

        if let Err(e) = values[1].parse::<i64>() {
            return Err(e.into());
        } else {
            max = values[1].parse().unwrap();
        }

        // Check min below max
        if min >= max {
            return Err(Error::msg("Range of values must be 100-200 for example..."));
        };
        Ok((min, Some(max)))
    } else {
        match s.parse::<i64>() {
            Ok(s) => Ok((s, None)),
            Err(e) => Err(e.into()),
        }
    }
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    let ctx = init_context(&cli).await?;

    if let Some(cmd) = &cli.command {
        cmd.run(&ctx).await?;
    }

    println!("Bye Bye!");

    Ok(())
}

async fn init_context(cli: &Cli) -> Result<Context> {
    // Get environment variables
    get_env_var(cli);

    // Initialize database pool
    let pool = connect().await?;
    POOL.get_or_init(|| pool.clone());

    // Get identity keys
    let identity_keys = User::get_identity_keys(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get identity keys: {}", e))?;
    IDENTITY_KEYS.get_or_init(|| identity_keys.clone());

    // Get trade keys
    let (trade_keys, trade_index) = User::get_next_trade_keys(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get trade keys: {}", e))?;
    TRADE_KEY.get_or_init(|| (trade_keys.clone(), trade_index));

    // Get Mostro admin keys
    let mostro_keys = Keys::from_str(
        &std::env::var("NSEC_PRIVKEY")
            .map_err(|e| anyhow::anyhow!("Failed to get mostro keys: {}", e))?,
    )?;
    MOSTRO_KEYS.get_or_init(|| mostro_keys.clone());
    MOSTRO_PUBKEY.get_or_init(|| mostro_keys.public_key());

    // Connect to Nostr relays
    let client = util::connect_nostr().await?;

    Ok(Context {
        client,
        identity_keys,
        trade_keys,
        trade_index,
        pool,
        mostro_keys: mostro_keys.clone(),
        mostro_pubkey: mostro_keys.public_key(),
    })
}

impl Commands {
    pub async fn run(&self, ctx: &Context) -> Result<()> {
        match self {
            // Simple order message commands
            Commands::FiatSent { order_id }
            | Commands::Release { order_id }
            | Commands::Dispute { order_id }
            | Commands::Cancel { order_id } => {
                crate::util::run_simple_order_msg(
                    self.clone(),
                    order_id,
                    &ctx.identity_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }

            // DM commands with pubkey parsing
            Commands::SendDm {
                pubkey,
                order_id,
                message,
            } => {
                execute_send_dm(PublicKey::from_str(pubkey)?, &ctx.client, order_id, message).await
            }
            Commands::DmToUser {
                pubkey,
                order_id,
                message,
            } => {
                execute_dm_to_user(PublicKey::from_str(pubkey)?, &ctx.client, order_id, message)
                    .await
            }
            Commands::AdmSendDm { pubkey, message } => {
                execute_adm_send_dm(PublicKey::from_str(pubkey)?, &ctx.client, message).await
            }
            Commands::ConversationKey { pubkey } => {
                execute_conversation_key(&ctx.trade_keys, PublicKey::from_str(pubkey)?).await
            }

            // Order management commands
            Commands::ListOrders {
                status,
                currency,
                kind,
            } => {
                execute_list_orders(
                    kind,
                    currency,
                    status,
                    ctx.mostro_pubkey,
                    &ctx.mostro_keys,
                    ctx.trade_index,
                    &ctx.pool,
                    &ctx.client,
                )
                .await
            }
            Commands::NewOrder {
                kind,
                fiat_code,
                amount,
                fiat_amount,
                payment_method,
                premium,
                invoice,
                expiration_days,
            } => {
                execute_new_order(
                    kind,
                    fiat_code,
                    fiat_amount,
                    amount,
                    payment_method,
                    premium,
                    invoice,
                    &ctx.identity_keys,
                    &ctx.trade_keys,
                    ctx.trade_index,
                    ctx.mostro_pubkey,
                    &ctx.client,
                    expiration_days,
                )
                .await
            }
            Commands::TakeSell {
                order_id,
                invoice,
                amount,
            } => {
                execute_take_sell(
                    order_id,
                    invoice,
                    *amount,
                    &ctx.identity_keys,
                    &ctx.trade_keys,
                    ctx.trade_index,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::TakeBuy { order_id, amount } => {
                execute_take_buy(
                    order_id,
                    *amount,
                    &ctx.identity_keys,
                    &ctx.trade_keys,
                    ctx.trade_index,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::AddInvoice { order_id, invoice } => {
                execute_add_invoice(
                    order_id,
                    invoice,
                    &ctx.identity_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::Rate { order_id, rating } => {
                execute_rate_user(
                    order_id,
                    rating,
                    &ctx.identity_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }

            // DM retrieval commands
            Commands::GetDm { since, from_user } => {
                execute_get_dm(
                    since,
                    ctx.trade_index,
                    &ctx.mostro_keys,
                    &ctx.client,
                    *from_user,
                    false,
                    &ctx.mostro_pubkey,
                )
                .await
            }
            Commands::GetDmUser { since } => {
                execute_get_dm_user(since, &ctx.client, &ctx.mostro_pubkey).await
            }
            Commands::GetAdminDm { since, from_user } => {
                execute_get_dm(
                    since,
                    ctx.trade_index,
                    &ctx.mostro_keys,
                    &ctx.client,
                    *from_user,
                    true,
                    &ctx.mostro_pubkey,
                )
                .await
            }

            // Admin commands
            Commands::AdmListDisputes {} => {
                execute_list_disputes(
                    ctx.mostro_pubkey,
                    &ctx.mostro_keys,
                    ctx.trade_index,
                    &ctx.pool,
                    &ctx.client,
                )
                .await
            }
            Commands::AdmAddSolver { npubkey } => {
                execute_admin_add_solver(
                    npubkey,
                    &ctx.mostro_keys,
                    &ctx.trade_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::AdmSettle { order_id } => {
                execute_admin_settle_dispute(
                    order_id,
                    &ctx.mostro_keys,
                    &ctx.trade_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::AdmCancel { order_id } => {
                execute_admin_cancel_dispute(
                    order_id,
                    &ctx.mostro_keys,
                    &ctx.trade_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }
            Commands::AdmTakeDispute { dispute_id } => {
                execute_take_dispute(
                    dispute_id,
                    &ctx.mostro_keys,
                    &ctx.trade_keys,
                    ctx.mostro_pubkey,
                    &ctx.client,
                )
                .await
            }

            // Simple commands
            Commands::Restore {} => {
                execute_restore(&ctx.identity_keys, ctx.mostro_pubkey, &ctx.client).await
            }
        }
    }
}
