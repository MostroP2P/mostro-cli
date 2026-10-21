use anyhow::{anyhow, Error, Result};
use clap::Parser;

use super::commands::Commands;
use super::invocation::{OutputMode, OUTPUT_LONG};

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
}

// Check range with two values value
pub(super) fn check_fiat_range(s: &str) -> Result<(i64, Option<i64>)> {
    if s.contains('-') {
        // Get values from CLI
        let values: Vec<&str> = s.split('-').collect();

        // Check if more than two values
        if values.len() > 2 {
            return Err(Error::msg("Wrong amount syntax"));
        };

        // Get ranged command
        let min = values[0]
            .parse::<i64>()
            .map_err(|e| anyhow!("Invalid min value: {}", e))?;
        let max = values[1]
            .parse::<i64>()
            .map_err(|e| anyhow!("Invalid max value: {}", e))?;

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
