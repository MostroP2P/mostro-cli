//! Dispatch of `order watch` in human mode, the only output that streams
//! (constitution 1.0.1). The arguments first, then configuration, the existing
//! identity and the store in order of cost, before any relay is contacted; the
//! work and transport settings and `--reply-timeout` are not read, since
//! nothing is sent to the node. Each update is rendered whole, written and
//! flushed as it comes; a stdout that cannot be written stops the watch. The
//! last line says why it stopped: the action that ended the trade, or the
//! `--timeout` deadline. Both exit 0. JSON is not offered yet: the dispatch
//! leaves it to the `output_not_supported` rejection.

use std::io::{self, Write};

use chrono::Utc;

use super::commands::WatchArgs;
use super::migrated::{code_name, diagnose, error_line};
use super::write::resolve_reader;
use super::Cli;
use crate::commands::order::watch::{order_watch, Follow, Watched};
use crate::msg::watch::WatchLimits;
use crate::outcome::watch::{WatchData, WatchInput, COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings, RUNTIME_ERROR_EXIT};
use crate::parse_human::watch::Shown;
use crate::parse_human::{render_watch_end, render_watch_update};

const UNWRITABLE: &str = "error: the watch could not be written";

/// Run a human `order watch` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &WatchArgs) -> i32 {
    let mut out = io::stdout();
    let (watched, written) = watch(cli, args, &mut out).await;
    let outcome = &watched.outcome;
    let mut err = io::stderr().lock();
    let exit = if outcome.error_code().is_some() {
        diagnose(&mut err, &error_line(outcome));
        outcome.exit_code()
    } else if written && write(&mut out, &render_watch_end(watched.ended_by.as_deref())) {
        outcome.exit_code()
    } else {
        diagnose(&mut err, UNWRITABLE);
        RUNTIME_ERROR_EXIT
    };
    if cli.verbose {
        let result = outcome
            .error_code()
            .map_or_else(|| "ok".to_string(), code_name);
        diagnose(&mut err, &format!("verbose: {COMMAND} {result}"));
    }
    exit
}

/// The watch, each update written to `out` as it comes, and whether every
/// update could be written.
async fn watch(cli: &Cli, args: &WatchArgs, out: &mut impl Write) -> (Watched, bool) {
    let (order, after) = (args.order, args.after.as_deref());
    let Ok(input) = WatchInput::validate(order, args.since, after, args.timeout, false) else {
        return (refused(ErrorCode::InvalidArguments), true);
    };
    let reader = match resolve_reader(cli, false).await {
        Ok(reader) => reader,
        Err(code) => return (refused(code), true),
    };
    let now = Utc::now();
    let since = input
        .start_cursor(u64::try_from(now.timestamp()).unwrap_or(0))
        .since();
    let (mut first, mut known, mut written) = (true, false, true);
    let mut sink = |data: &WatchData| {
        let peer = data.counterparty_pubkey().is_some();
        let shown = if first {
            Shown::First { since }
        } else {
            Shown::Later {
                learned: peer && !known,
            }
        };
        (first, known) = (false, peer);
        written = write(out, &render_watch_update(data, shown));
        written
    };
    let (config, store) = (&reader.config, &reader.store);
    let limits = WatchLimits::default();
    let follow = Follow::UntilEnd(&mut sink);
    let watched = order_watch(&input, config, store, &limits, now, follow).await;
    (watched, written)
}

/// Write `text` whole and flush it; `false` when stdout refused it.
fn write(out: &mut impl Write, text: &str) -> bool {
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .is_ok()
}

/// A failure before any relay was contacted.
fn refused(code: ErrorCode) -> Watched {
    Watched {
        outcome: Envelope::error(
            Some(COMMAND),
            code,
            Warnings::new(),
            ExecutionMeta::read(None),
        ),
        ended_by: None,
    }
}
