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
    /// Requests open orders from mostro pubkey
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
    /// Take a sell order from a mostro pubkey
    TakeSell {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
    /// Take a buy order from a mostro pubkey
    TakeBuy {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Get the list of mostro direct message since the last hour - used to check order state.
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
    /// Create a new buy/sell order on mostro
    Neworder {
        /// Choose an order kind
        #[arg(value_enum)]
        #[arg(short, long)]
        #[clap(default_value = "sell")]
        kind_order: Option<Kind>,
        /// Currency selected
        #[arg(short, long)]
        fiat_code: String,
        /// Sats amount
        #[arg(short, long)]
        amount: u32,
        /// Fiat amount
        #[arg(short = 'm', long)]
        fiat_amount: u32,
        /// Payment method
        #[arg(short, long)]
        payment_method: String,
        /// Premium on price
        #[arg(short = 'r', long)]
        #[clap(default_value_t = 0)]
        prime: i8,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
}
