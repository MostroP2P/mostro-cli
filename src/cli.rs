pub mod add_invoice;
pub mod get_dm;
pub mod list_orders;
pub mod new_order;
pub mod rate_user;
pub mod send_msg;
pub mod take_buy;
pub mod take_sell;

use clap::{Parser, Subcommand};

use mostro_core::{Kind, Status};
use uuid::Uuid;

use std::env::{set_var, var};

use anyhow::Result;

use nostr_sdk::prelude::FromBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;

use crate::cli::add_invoice::execute_add_invoice;
use crate::cli::get_dm::execute_get_dm;
use crate::cli::list_orders::execute_list_orders;
use crate::cli::new_order::execute_new_order;
use crate::cli::rate_user::execute_rate_user;
use crate::cli::send_msg::execute_send_msg;
use crate::cli::take_buy::execute_take_buy;
use crate::cli::take_sell::execute_take_sell;
use crate::util;

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
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum Commands {
    /// Requests open orders from Mostro pubkey
    ListOrders {
        /// Status of the order
        #[arg(short, long)]
        #[clap(default_value = "pending")]
        status: Option<Status>,
        /// Currency selected
        #[arg(short, long)]
        currency: Option<String>,
        /// Choose an order kind
        #[arg(value_enum)]
        #[arg(short, long)]
        kind: Option<Kind>,
    },
    /// Create a new buy/sell order on Mostro
    Neworder {
        /// Choose an order kind
        #[arg(value_enum)]
        #[arg(short, long)]
        kind: Kind,
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
        fiat_amount: i64,
        /// Payment method
        #[arg(short = 'm', long)]
        payment_method: String,
        /// Premium on price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        premium: i64,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
    },
    /// Take a sell order from a Mostro pubkey
    TakeSell {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
    },
    /// Take a buy order from a Mostro pubkey
    TakeBuy {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Take a buy order from a Mostro pubkey
    AddInvoice {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
    /// Get the list of Mostro direct messages since the last hour, used to check order state
    GetDm {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
    },
    /// Send fiat sent message to confirm payment to other user
    FiatSent {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Settle the hold invoice and pay to buyer.
    Release {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Cancel a pending order
    Cancel {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Rate counterpart after a successful trade
    Rate {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
        /// Rating from 1 to 5
        #[arg(short, long)]
        rating: u64,
    },
}

/// Check range simple version for just a single value
pub fn check_fiat_range(s: &str) -> Result<i64, String> {
    match s.parse::<i64>() {
        Ok(val) => Ok(val),
        Err(_e) => Err(String::from("Error on parsing sats value")),
    }
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
    }
    // Mostro pubkey
    let pubkey = var("MOSTRO_PUBKEY").expect("$MOSTRO_PUBKEY env var needs to be set");
    let mostro_key = XOnlyPublicKey::from_bech32(pubkey)?;

    // My key
    let my_key = util::get_keys()?;

    // Call function to connect to relays
    let client = util::connect_nostr().await?;

    if let Some(cmd) = cli.command {
        match &cmd {
            Commands::ListOrders {
                status,
                currency,
                kind,
            } => execute_list_orders(kind, currency, status, mostro_key, &client).await?,
            Commands::TakeSell { order_id, invoice } => {
                execute_take_sell(order_id, invoice, &my_key, mostro_key, &client).await?
            }
            Commands::TakeBuy { order_id } => {
                execute_take_buy(order_id, &my_key, mostro_key, &client).await?
            }
            Commands::AddInvoice { order_id, invoice } => {
                execute_add_invoice(order_id, invoice, &my_key, mostro_key, &client).await?
            }
            Commands::GetDm { since } => {
                execute_get_dm(since, &my_key, mostro_key, &client).await?
            }
            Commands::FiatSent { order_id }
            | Commands::Release { order_id }
            | Commands::Cancel { order_id } => {
                execute_send_msg(cmd.clone(), order_id, &my_key, mostro_key, &client).await?
            }
            Commands::Neworder {
                kind,
                fiat_code,
                amount,
                fiat_amount,
                payment_method,
                premium,
                invoice,
            } => {
                execute_new_order(
                    kind,
                    fiat_code,
                    fiat_amount,
                    amount,
                    payment_method,
                    premium,
                    invoice,
                    &my_key,
                    mostro_key,
                    &client,
                )
                .await?
            }
            Commands::Rate { order_id, rating } => {
                execute_rate_user(order_id, rating, &my_key, mostro_key, &client).await?;
            }
        };
    }

    println!("Bye Bye!");

    Ok(())
}
