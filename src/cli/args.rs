use clap::{value_parser, Parser};

use super::commands::Commands;
use super::invocation::{OutputMode, OUTPUT_LONG};

/// The long name of the reply deadline option.
pub const REPLY_TIMEOUT_LONG: &str = "reply-timeout";
/// The reply deadline when the option is not given, in seconds.
pub const DEFAULT_REPLY_TIMEOUT: u64 = 15;

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
    pub mostro_pubkey: Option<String>,
    #[arg(short, long)]
    pub relays: Option<String>,
    #[arg(short, long)]
    pub pow: Option<String>,
    #[arg(short, long)]
    pub secret: bool,
    /// Wire transport to speak to the node. Only `nip44` (protocol v2) is
    /// supported. `gift-wrap` (protocol v1) is rejected. When omitted, the CLI
    /// auto-detects from the node's `protocol_version` tag and defaults to
    /// nip44.
    #[arg(short, long)]
    pub transport: Option<String>,
    /// Output format, accepted at any command depth. Defaults to `human`;
    /// only migrated commands support `json`.
    #[arg(long = OUTPUT_LONG, global = true, value_enum)]
    pub output: Option<OutputMode>,
    /// Seconds to wait for the node's reply (5 to 300, default 15), accepted at
    /// any command depth. Applies to the migrated commands that wait for a
    /// reply; the public reads keep their own limits and the legacy commands
    /// ignore it
    #[arg(
        long = REPLY_TIMEOUT_LONG,
        global = true,
        value_name = "SECONDS",
        default_value_t = DEFAULT_REPLY_TIMEOUT,
        value_parser = value_parser!(u64).range(5..=300)
    )]
    pub reply_timeout: u64,
}
