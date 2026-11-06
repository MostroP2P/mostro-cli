//! Dispatch of `identity init` and `identity show`. They read no node
//! configuration, connect to nothing, ask nothing and read no stdin; the data
//! directory (`MCLI_DIR`, else `~/.mcli`) is all they need. Presentation is the
//! shared one of the migrated commands ([`super::write::finish`]).

use super::commands::IdentityCommands;
use super::invocation::OutputMode;
use super::write::finish;
use super::Cli;
use crate::commands::identity::{identity_init, identity_show};
use crate::identity::configured_dir;
use crate::parse_human::{render_identity_init, render_identity_show};

const UNWRITABLE: &str = "error: the identity could not be written";
/// Neither command is ever `uncertain`, so no advice follows an outcome.
const NO_ADVICE: &str = "";

/// Run `identity init` or `identity show` and return its exit status.
pub(super) async fn run(cli: &Cli, command: &IdentityCommands, output: OutputMode) -> i32 {
    let dir = configured_dir();
    match command {
        IdentityCommands::Init => {
            let outcome = identity_init(dir.as_deref()).await;
            finish(
                cli,
                output,
                &outcome,
                render_identity_init,
                (UNWRITABLE, NO_ADVICE),
            )
        }
        IdentityCommands::Show => {
            let outcome = identity_show(dir.as_deref()).await;
            finish(
                cli,
                output,
                &outcome,
                render_identity_show,
                (UNWRITABLE, NO_ADVICE),
            )
        }
    }
}
