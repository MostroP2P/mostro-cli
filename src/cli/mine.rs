//! Dispatch of `order mine`: the local store only, read-only. No node
//! configuration, relay, stdin or confirmation. Presentation is the shared one
//! of the migrated commands ([`super::write::finish`]).

use std::io;

use super::invocation::OutputMode;
use super::migrated::diagnose;
use super::write::finish;
use super::Cli;
use crate::commands::order::mine::order_mine;
use crate::outcome::WarningCode;
use crate::parse_human::render_mine;
use crate::store::Store;

const UNWRITABLE: &str = "error: the order list could not be written";
/// `order mine` is never `uncertain`, so no advice follows an outcome.
const NO_ADVICE: &str = "";
/// `trade_keys_limited` for a human, who cannot read it in a document.
const KEYS_LIMITED: &str = "warning: trade keys older than the newest 200 were not derived; \
                            their orders show index unknown.";

/// Run `order mine` and return its exit status.
pub(super) async fn run(cli: &Cli, output: OutputMode) -> i32 {
    let store = Store::configured().ok();
    let outcome = order_mine(store.as_ref()).await;
    if output == OutputMode::Human && outcome.warnings().contains(WarningCode::TradeKeysLimited) {
        diagnose(&mut io::stderr().lock(), KEYS_LIMITED);
    }
    finish(cli, output, &outcome, render_mine, (UNWRITABLE, NO_ADVICE))
}
