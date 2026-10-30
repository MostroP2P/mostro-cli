//! The part of the dispatch that every migrated write command shares: resolve
//! configuration, identity and store in order of cost, ask for confirmation in
//! human mode only, and present the outcome. A command supplies its own input
//! validation, preview, operation and renderer. Everything that can fail
//! locally fails before anything is published.

use std::env;
use std::io::{self, BufRead, Write};

use nostr_sdk::prelude::Keys;
use serde::Serialize;

use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose, request_options};
use super::Cli;
use crate::commands::order::WriteOptions;
use crate::identity::{load_identity_keys, IdentityError};
use crate::msg::PublicReadConfig;
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, Status,
    Warnings, RUNTIME_ERROR_EXIT,
};
use crate::parse_machine::emit;
use crate::store::{Store, StoreError, Trade};

/// Configuration, identity, store and the next trade, all checked.
pub(super) struct Resolved {
    pub config: PublicReadConfig,
    pub options: WriteOptions,
    pub identity: Keys,
    pub store: Store,
    pub trade: Trade,
}

/// Resolve configuration, then the existing identity (read-only), then the
/// store (it must accept a write) and the key and index of the next trade.
pub(super) async fn resolve(cli: &Cli) -> Result<Resolved, ErrorCode> {
    let getenv = |name: &str| env::var(name).ok();
    let config =
        PublicReadConfig::resolve(cli.mostro_pubkey.as_deref(), cli.relays.as_deref(), getenv)?;
    let request = request_options(cli.pow.as_deref(), cli.transport.as_deref(), getenv)?;
    let identity = load_identity_keys().await.map_err(|error| match error {
        IdentityError::Missing => ErrorCode::IdentityMissing,
        IdentityError::Unreadable => ErrorCode::IdentityUnreadable,
    })?;
    let store = Store::configured().map_err(store_code)?;
    store.ensure_writable().await.map_err(store_code)?;
    let trade = store.next_trade().await.map_err(store_code)?;
    Ok(Resolved {
        config,
        options: WriteOptions {
            pow: request.pow,
            check_protocol: request.check_protocol,
        },
        identity,
        store,
        trade,
    })
}

fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable => ErrorCode::StoreUnwritable,
    }
}

/// JSON never prompts: without `--yes` it does nothing at all.
pub(super) fn needs_yes(output: OutputMode, yes: bool) -> bool {
    output == OutputMode::Json && !yes
}

/// A failure before anything was published or written.
pub(super) fn refuse<D>(command: &'static str, code: ErrorCode) -> Envelope<D, ()> {
    Envelope::error(
        Some(command),
        code,
        Warnings::new(),
        ExecutionMeta::write(
            Publication::NotAttempted,
            DaemonAcceptance::Unknown,
            Persistence::NotAttempted,
        ),
    )
}

/// Show `preview` on stdout and ask `question` on stderr; anything but blank
/// or `y` (and a closed stdin) declines.
pub(super) fn confirmed(preview: &str, question: &str) -> bool {
    let mut out = io::stdout().lock();
    if out
        .write_all(preview.as_bytes())
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let mut err = io::stderr().lock();
    let _ = write!(err, "{question} [Y/n] ");
    let _ = err.flush();
    drop(err);
    accepts(io::stdin().lock())
}

/// Blank, `y` and `Y` accept, with trailing spaces ignored; end of input and
/// every other answer decline.
fn accepts(mut input: impl BufRead) -> bool {
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(read) if read > 0 => matches!(line.trim_end().to_lowercase().as_str(), "" | "y"),
        _ => false,
    }
}

/// Present `outcome` in `output` mode and return the exit status, with the
/// `--verbose` line when asked. `render` is the command's human result,
/// `unwritable` the fixed line for a stdout that cannot be written and `advice`
/// the line that follows an uncertain outcome.
pub(super) fn finish<D: Serialize>(
    cli: &Cli,
    output: OutputMode,
    outcome: &Envelope<D, ()>,
    render: impl Fn(&D) -> String,
    (unwritable, advice): (&str, &str),
) -> i32 {
    let exit = match output {
        OutputMode::Json => emit(outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
        OutputMode::Human => print_human(
            outcome,
            render,
            (unwritable, advice),
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        ),
    };
    if cli.verbose {
        diagnose(&mut io::stderr().lock(), &summary(outcome));
    }
    exit
}

/// The only `--verbose` line of a write command: its name and a code, never
/// the order, key, relay, invoice or node text.
fn summary<D>(outcome: &Envelope<D, ()>) -> String {
    let result = outcome
        .error_code()
        .map_or_else(|| "ok".to_string(), code_name);
    format!(
        "verbose: {} {result}",
        outcome.command().unwrap_or("unresolved")
    )
}

/// The human result on `out`; errors, and the guidance of an uncertain
/// outcome, as fixed lines on `err`. Rendered completely before the first byte.
fn print_human<D>(
    outcome: &Envelope<D, ()>,
    render: impl Fn(&D) -> String,
    (unwritable, advice): (&str, &str),
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    if let Some(code) = outcome.error_code() {
        let line = match outcome.reason() {
            Some(reason) => format!("error: {} (reason: {reason})", code.message()),
            None => format!("error: {}", code.message()),
        };
        diagnose(err, &line);
        if outcome.status() == Status::Uncertain {
            diagnose(err, advice);
        }
    }
    let Some(data) = outcome.data() else {
        return outcome.exit_code();
    };
    match out
        .write_all(render(data).as_bytes())
        .and_then(|()| out.flush())
    {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, unwritable);
            RUNTIME_ERROR_EXIT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_blank_and_y_accept_and_end_of_input_declines() {
        for yes in ["\n", "\r\n", "y\n", "Y\n", "y\r\n", "  \n", "y  \n"] {
            assert!(accepts(yes.as_bytes()), "{yes:?}");
        }
        for no in ["", "n\n", "N\n", "n\r\n", "maybe\n", " y\n", "yes\n"] {
            assert!(!accepts(no.as_bytes()), "{no:?}");
        }
    }

    #[test]
    fn json_needs_yes_and_human_never_does() {
        assert!(needs_yes(OutputMode::Json, false));
        assert!(!needs_yes(OutputMode::Json, true));
        assert!(!needs_yes(OutputMode::Human, false));
    }
}
