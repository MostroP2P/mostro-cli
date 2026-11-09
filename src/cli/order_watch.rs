//! Dispatch of `order watch`. The arguments first, then configuration, the
//! existing identity and the store in order of cost, before any relay is
//! contacted; the work and transport settings and `--reply-timeout` are not
//! read, since nothing is sent to the node.
//!
//! With `--output json` a call writes one document: at the first new records
//! after the replays, or empty at its deadline (`--timeout`, 30 s by default),
//! with the cursor to pass back with `--after`. Human mode is the only output
//! that streams (constitution 1.0.1): each update is rendered whole, written
//! and flushed as it comes; a stdout that cannot be written stops the watch,
//! and on Unix so does a reader that closes it while the watch is idle. Its
//! last line says why it stopped: the action that ended the trade, or the
//! `--timeout` deadline. Both exit 0.

use std::cell::Cell;
use std::io::{self, Write};
use std::pin::pin;

use chrono::Utc;

use super::commands::WatchArgs;
use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose, error_line};
use super::write::{resolve_reader, Reader};
use super::Cli;
use crate::commands::order::watch::{order_watch, Follow, WatchOutcome, Watched};
use crate::msg::watch::WatchLimits;
use crate::outcome::watch::{WatchData, WatchInput, COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings, RUNTIME_ERROR_EXIT};
use crate::parse_human::watch::Shown;
use crate::parse_human::{render_watch_end, render_watch_update};
use crate::parse_machine::emit;

const UNWRITABLE: &str = "error: the watch could not be written";

/// How often an idle watch checks whether stdout's reader is gone.
#[cfg(unix)]
const GONE_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Run `order watch` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &WatchArgs, output: OutputMode) -> i32 {
    match output {
        OutputMode::Json => json(cli, args).await,
        OutputMode::Human => human(cli, args).await,
    }
}

/// One JSON document: the first new records, or an empty success at the
/// deadline.
async fn json(cli: &Cli, args: &WatchArgs) -> i32 {
    let watched = match prepare(cli, args, true).await {
        Ok((input, reader)) => {
            let (config, store) = (&reader.config, &reader.store);
            let limits = WatchLimits::default();
            order_watch(
                &input,
                config,
                store,
                &limits,
                Utc::now(),
                Follow::FirstChange,
            )
            .await
        }
        Err(refused) => refused,
    };
    let outcome = &watched.outcome;
    let exit = emit(outcome, &mut io::stdout().lock(), &mut io::stderr().lock());
    verbose(cli, outcome);
    exit
}

/// Stream the updates, then the end, deadline or error line.
async fn human(cli: &Cli, args: &WatchArgs) -> i32 {
    let mut out = io::stdout();
    let (watched, written) = watch(cli, args, &mut out).await;
    let outcome = &watched.outcome;
    let exit = if outcome.error_code().is_some() {
        diagnose(&mut io::stderr().lock(), &error_line(outcome));
        outcome.exit_code()
    } else if written && write(&mut out, &render_watch_end(watched.ended_by.as_deref())) {
        outcome.exit_code()
    } else {
        diagnose(&mut io::stderr().lock(), UNWRITABLE);
        RUNTIME_ERROR_EXIT
    };
    verbose(cli, outcome);
    exit
}

/// The `--verbose` line: the command and its result code, nothing else.
fn verbose(cli: &Cli, outcome: &WatchOutcome) {
    if cli.verbose {
        let result = outcome
            .error_code()
            .map_or_else(|| "ok".to_string(), code_name);
        diagnose(
            &mut io::stderr().lock(),
            &format!("verbose: {COMMAND} {result}"),
        );
    }
}

/// The validated input and what the watch reads, or the failure that comes
/// before any relay is contacted.
async fn prepare(cli: &Cli, args: &WatchArgs, json: bool) -> Result<(WatchInput, Reader), Watched> {
    let (order, after) = (args.order, args.after.as_deref());
    let Ok(input) = WatchInput::validate(order, args.since, after, args.timeout, json) else {
        return Err(refused(ErrorCode::InvalidArguments));
    };
    let reader = resolve_reader(cli, false).await.map_err(refused)?;
    Ok((input, reader))
}

/// The watch, each update written to `out` as it comes, and whether every
/// update could be written and stdout's reader stayed.
async fn watch(cli: &Cli, args: &WatchArgs, out: &mut impl Write) -> (Watched, bool) {
    let (input, reader) = match prepare(cli, args, false).await {
        Ok(prepared) => prepared,
        Err(refused) => return (refused, true),
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
    let gone = Cell::new(false);
    let reader_gone = pin!(async {
        stdout_gone().await;
        gone.set(true);
    });
    let follow = Follow::UntilEnd(&mut sink, reader_gone);
    let watched = order_watch(&input, config, store, &limits, now, follow).await;
    (watched, written && !gone.get())
}

/// Resolves once stdout's reader is gone. On Unix, polling stdout then
/// reports an error or a hang-up: the write end of a pipe whose reader closed
/// reports `POLLERR`. Elsewhere it never resolves, and a closed stdout stops
/// the watch at its next write.
async fn stdout_gone() {
    #[cfg(unix)]
    {
        let mut tick = tokio::time::interval(GONE_POLL);
        loop {
            tick.tick().await;
            let mut stdout = libc::pollfd {
                fd: libc::STDOUT_FILENO,
                events: 0,
                revents: 0,
            };
            // SAFETY: `stdout` is one valid `pollfd` for the whole call, and a
            // zero timeout never blocks.
            let ready = unsafe { libc::poll(&mut stdout, 1, 0) };
            if ready > 0 && stdout.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                return;
            }
        }
    }
    #[cfg(not(unix))]
    std::future::pending::<()>().await;
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
