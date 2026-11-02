//! Dispatch of `order sync`: configuration, identity and a writable store are
//! resolved in order of cost before any relay is contacted; then one bounded
//! read and the rows it calls for. Nothing is published, so no confirmation is
//! asked: it only saves rows for orders the node announced to the user's own
//! keys, and never replaces one.

use std::io::{self, Write};

use chrono::Utc;

use super::commands::SyncArgs;
use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose};
use super::write::resolve_reader;
use super::Cli;
use crate::commands::order::sync::{order_sync, SyncOutcome, COMMAND};
use crate::msg::ReadLimits;
use crate::outcome::sync::SyncInput;
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Status, Warnings, RUNTIME_ERROR_EXIT};
use crate::parse_human::render_sync;
use crate::parse_machine::emit;

const UNWRITABLE: &str = "error: the sync result could not be written";
const ADVICE: &str =
    "Some orders were saved and one was not; run `order sync` again, it saves only what is missing.";

/// Run `order sync` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &SyncArgs, output: OutputMode) -> i32 {
    let outcome = outcome(cli, args).await;
    let exit = match output {
        OutputMode::Json => emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
        OutputMode::Human => print(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
    };
    if cli.verbose {
        let result = outcome
            .error_code()
            .map_or_else(|| "ok".to_string(), code_name);
        diagnose(
            &mut io::stderr().lock(),
            &format!("verbose: {COMMAND} {result}"),
        );
    }
    exit
}

async fn outcome(cli: &Cli, args: &SyncArgs) -> SyncOutcome {
    let Ok(input) = SyncInput::validate(args.since) else {
        return refused(ErrorCode::InvalidArguments);
    };
    let reader = match resolve_reader(cli, true).await {
        Ok(reader) => reader,
        Err(code) => return refused(code),
    };
    order_sync(
        &input,
        &reader.config,
        &reader.store,
        &ReadLimits::default(),
        Utc::now(),
    )
    .await
}

/// A failure before any relay was contacted.
fn refused(code: ErrorCode) -> SyncOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::read(None),
    )
}

/// The human result on `out`, or fixed lines on `err`. Rendered completely
/// before the first byte is written.
fn print(outcome: &SyncOutcome, out: &mut impl Write, err: &mut impl Write) -> i32 {
    if let Some(code) = outcome.error_code() {
        diagnose(err, &format!("error: {}", code.message()));
        if outcome.status() == Status::Uncertain {
            diagnose(err, ADVICE);
        }
    }
    let Some(data) = outcome.data() else {
        return outcome.exit_code();
    };
    match out
        .write_all(render_sync(data).as_bytes())
        .and_then(|()| out.flush())
    {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, UNWRITABLE);
            RUNTIME_ERROR_EXIT
        }
    }
}
