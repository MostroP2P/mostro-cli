//! Dispatch of `order mine`: the local store only, read-only. No node
//! configuration, relay, stdin or confirmation. Presentation is the shared one
//! of the migrated commands ([`super::write::finish`]).

use super::invocation::OutputMode;
use super::write::finish;
use super::Cli;
use crate::commands::order::mine::order_mine;
use crate::parse_human::render_mine;
use crate::store::Store;

const UNWRITABLE: &str = "error: the order list could not be written";
/// `order mine` is never `uncertain`, so no advice follows an outcome.
const NO_ADVICE: &str = "";

/// Run `order mine` and return its exit status.
pub(super) async fn run(cli: &Cli, output: OutputMode) -> i32 {
    let store = Store::configured().ok();
    let outcome = order_mine(store.as_ref()).await;
    finish(cli, output, &outcome, render_mine, (UNWRITABLE, NO_ADVICE))
}
