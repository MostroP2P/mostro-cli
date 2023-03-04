use clap::{Parser, Subcommand};

use crate::types::{Kind, Status};
use uuid::Uuid;

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

#[derive(Subcommand)]
#[clap(rename_all = "lower")]
pub enum Commands {
    /// Requests open orders from Mostro pubkey
    ListOrders {
        /// Status of the order
        #[arg(short, long)]
        #[clap(default_value = "pending")]
        order_status: Option<Status>,
        /// Currency selected
        #[arg(short, long)]
        currency: Option<String>,
        /// Choose an order kind
        #[arg(value_enum)]
        #[arg(short, long)]
        kind_order: Option<Kind>,
    },
    /// Create a new buy/sell order on Mostro
    Neworder {
        /// Choose an order kind
        #[arg(value_enum)]
        #[arg(short, long)]
        kind: Option<Kind>,
        /// Sats amount
        #[arg(short, long)]
        amount: u32,
        /// Currency selected
        #[arg(short = 'c', long)]
        fiat_code: String,
        /// Fiat amount
        #[arg(short, long)]
        fiat_amount: u32,
        /// Payment method
        #[arg(short = 'm', long)]
        payment_method: String,
        /// Premium on price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        prime: i8,
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
        invoice: String,
    },
    /// Take a buy order from a Mostro pubkey
    TakeBuy {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
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
}
