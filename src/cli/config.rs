//! Dispatch of `config set`: the config file of the data directory
//! (`MCLI_DIR`, else `~/.mcli`), with no node configuration, relay, stdin or
//! confirmation. Presentation is the shared one of the migrated commands
//! ([`super::write::finish`]).

use std::io;

use super::commands::ConfigCommands;
use super::invocation::OutputMode;
use super::migrated::diagnose;
use super::write::finish;
use super::Cli;
use crate::commands::config::{config_set, invalid_input};
use crate::identity::configured_dir;
use crate::outcome::config::ConfigSetInput;
use crate::parse_human::render_config;

const UNWRITABLE: &str = "error: the config set result could not be written";
/// `config set` is never `uncertain`, so no advice follows an outcome.
const NO_ADVICE: &str = "";

/// Run a `config` command and return its exit status. A refused key or value
/// is never echoed; a refused or unreachable existing file is named, with its
/// line, on one `hint:` line and left untouched.
pub(super) fn run(cli: &Cli, command: &ConfigCommands, output: OutputMode) -> i32 {
    let ConfigCommands::Set(args) = command;
    let outcome = match ConfigSetInput::new(&args.key, &args.value) {
        None => invalid_input(),
        Some(input) => {
            let (outcome, problem) = config_set(configured_dir().as_deref(), &input);
            if let Some(problem) = problem {
                diagnose(&mut io::stderr().lock(), &format!("hint: {problem}"));
            }
            outcome
        }
    };
    finish(
        cli,
        output,
        &outcome,
        render_config,
        (UNWRITABLE, NO_ADVICE),
    )
}
