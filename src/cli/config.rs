//! Dispatch of `config set` and `config show`: the config file of the data
//! directory (`MCLI_DIR`, else `~/.mcli`), with no node configuration, relay,
//! stdin or confirmation. Presentation is the shared one of the migrated
//! commands ([`super::write::finish`]).

use std::io;

use super::commands::ConfigCommands;
use super::invocation::OutputMode;
use super::migrated::diagnose;
use super::write::finish;
use super::Cli;
use crate::commands::config::{config_set, config_show, invalid_input};
use crate::identity::configured_dir;
use crate::outcome::config::ConfigSetInput;
use crate::parse_human::render_config;

const SET_UNWRITABLE: &str = "error: the config set result could not be written";
const SHOW_UNWRITABLE: &str = "error: the config show result could not be written";
/// Neither command is ever `uncertain`, so no advice follows an outcome.
const NO_ADVICE: &str = "";

/// Run a `config` command and return its exit status. A refused key or value
/// is never echoed; a refused or unreachable existing file is named, with its
/// line, on one `hint:` line, and `config set` leaves it untouched.
pub(super) fn run(cli: &Cli, command: &ConfigCommands, output: OutputMode) -> i32 {
    let dir = configured_dir();
    let (outcome, problem, unwritable) = match command {
        ConfigCommands::Set(args) => match ConfigSetInput::new(&args.key, &args.value) {
            None => (invalid_input(), None, SET_UNWRITABLE),
            Some(input) => {
                let (outcome, problem) = config_set(dir.as_deref(), &input);
                (outcome, problem, SET_UNWRITABLE)
            }
        },
        ConfigCommands::Show => {
            let (outcome, problem) = config_show(dir.as_deref());
            (outcome, problem, SHOW_UNWRITABLE)
        }
    };
    if let Some(problem) = problem {
        diagnose(&mut io::stderr().lock(), &format!("hint: {problem}"));
    }
    finish(
        cli,
        output,
        &outcome,
        render_config,
        (unwritable, NO_ADVICE),
    )
}
