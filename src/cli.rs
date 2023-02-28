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
    /// Take an order from a mostro pubkey
    TakeSell {
        /// Order id number
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
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
}
