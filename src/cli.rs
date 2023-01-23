use clap::{Parser, Subcommand};

use crate::types::Kind;

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
    /// Requests open orders from mostro pubkey ()
    Listorders {
        #[clap(default_value = "Pending")]
        orderstatus: String,
        #[arg(short, long)]
        #[clap(default_value = "ALL")]
        currency: String,
        #[arg(value_enum)]
        #[arg(short, long)]
        kindorder: Option<Kind>,
    },
}
