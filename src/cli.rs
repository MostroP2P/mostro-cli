pub mod add_bond_invoice;
pub mod add_invoice;
pub mod adm_send_dm;
pub mod conversation_key;
pub mod dispute_chat;
pub mod dm_to_user;
pub mod get_dm;
pub mod get_dm_user;
pub mod invocation;
pub mod last_trade_index;
pub mod list_disputes;
pub mod maintenance;
pub mod new_order;
pub mod orders_info;
pub mod rate_user;
pub mod restore;
pub mod send_admin_dm_attach;
pub mod send_dm;
pub mod send_msg;
pub mod take_dispute;
pub mod take_order;
pub mod user_info;

mod args;
mod commands;
mod context;
mod legacy;
mod registry;

pub use args::Cli;
pub use commands::{Commands, OrderCommands};
pub use context::Context;
pub use registry::{lookup, CommandEntry, JsonSupport, REGISTRY};

use std::env;
use std::io::{self, Write};

use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::error::ErrorKind;
use clap::{ArgMatches, CommandFactory, FromArgMatches};

use crate::commands::order::list::{list_orders, ListOutcome, COMMAND};
use crate::msg::{PublicReadConfig, ReadLimits};
use crate::outcome::{
    Envelope, ErrorCode, ExecutionMeta, Warnings, ARGUMENT_ERROR_EXIT, RUNTIME_ERROR_EXIT,
};
use crate::parse_human::render_list;
use crate::parse_machine::emit;
use crate::util::net::install_rustls_crypto_provider;
use context::init_context;
use invocation::{parse_invocation, ArgumentFailure, Invocation, OutputMode, OutputSelection};

/// Parse argv, run the command and return the process exit status.
pub async fn run() -> Result<i32> {
    let (matches, output) = match parse_invocation(Cli::command(), env::args_os()) {
        Invocation::Parsed { matches, output } => (matches, output),
        Invocation::Text(display) => {
            let _ = display.print();
            return Ok(display.exit_code());
        }
        Invocation::Rejected(failure) => return Ok(report(&failure)),
    };
    let cli = Cli::from_arg_matches(&matches)
        .map_err(|_| anyhow!("internal error: parsed arguments could not be bound"))?;

    // Public listing needs no identity, database, environment mutation or RPC.
    if let Some(Commands::Order {
        command:
            OrderCommands::List {
                side,
                currency,
                status,
            },
    }) = &cli.command
    {
        let outcome = list_orders(
            side.as_deref(),
            currency.as_deref(),
            status.as_deref(),
            || {
                PublicReadConfig::resolve(
                    cli.mostro_pubkey.as_deref(),
                    cli.relays.as_deref(),
                    |name| env::var(name).ok(),
                )
            },
            &ReadLimits::default(),
            Utc::now(),
        )
        .await;
        let exit = match output {
            OutputMode::Json => emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
            OutputMode::Human => {
                print_list(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
            }
        };
        if cli.verbose {
            diagnose(&mut io::stderr().lock(), &verbose_summary(&outcome));
        }
        return Ok(exit);
    }
    if output == OutputMode::Json {
        return Ok(reject_json(&matches));
    }
    install_rustls_crypto_provider();

    // Daemon-local gRPC commands: no relays, keys or database involved.
    if let Some(cmd) = &cli.command {
        if let Some(result) = cmd.run_rpc().await {
            return result.map(|()| 0);
        }
    }

    let ctx = init_context(&cli).await?;

    if let Some(cmd) = &cli.command {
        cmd.run(&ctx).await?;
    }

    Ok(0)
}

/// The only `--verbose` line of the public listing: counts and codes, never
/// relay, URL, argument or SDK text. No logger is installed on this path.
fn verbose_summary(outcome: &ListOutcome) -> String {
    let result = outcome
        .error_code()
        .map_or_else(|| "ok".to_string(), code_name);
    match outcome.snapshot() {
        Some(snapshot) => format!(
            "verbose: {COMMAND} {result}; {} events from {}/{} relays",
            snapshot.received_events, snapshot.completed_relays, snapshot.configured_relays
        ),
        None => format!("verbose: {COMMAND} {result}; no read"),
    }
}

/// The wire name of `code`, as serialized in the JSON outcome.
fn code_name(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_else(|| "internal_error".to_string())
}

/// One diagnostic line on stderr. A stderr that cannot be written is ignored:
/// the exit status already carries the result and a panic would replace it.
fn diagnose(err: &mut impl Write, line: &str) {
    let _ = writeln!(err, "{line}");
}

/// The human listing on `out`, or a fixed error line on `err`. The listing is
/// rendered completely before the first byte is written; a failed write exits
/// 1 and appends nothing.
fn print_list(outcome: &ListOutcome, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let Some(data) = outcome.data() else {
        let message = outcome
            .error_code()
            .unwrap_or(ErrorCode::InternalError)
            .message();
        diagnose(err, &format!("error: {message}"));
        return outcome.exit_code();
    };
    let text = render_list(data);
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, "error: the order list could not be written");
            RUNTIME_ERROR_EXIT
        }
    }
}

/// The subcommand names from the root to the selected leaf.
fn command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut level = matches;
    while let Some((name, sub)) = level.subcommand() {
        path.push(name.to_string());
        level = sub;
    }
    path
}

/// JSON for an accepted invocation of a command without JSON support. This
/// answers before RPC routing, environment access or context initialization.
/// Root options without a command name no command and are invalid.
fn reject_json(matches: &ArgMatches) -> i32 {
    let path = command_path(matches);
    if path.is_empty() {
        rejection(None, ErrorCode::InvalidArguments)
    } else {
        rejection(lookup(&path), ErrorCode::OutputNotSupported)
    }
}

/// An argument error: a JSON envelope when JSON was clearly selected,
/// otherwise fixed text on stderr.
fn report(failure: &ArgumentFailure) -> i32 {
    if let Some(code) = failure.json_error_code() {
        let entry = failure.command.as_deref().and_then(lookup);
        return rejection(entry, code);
    }
    let bare = failure.kind == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        && failure.output == OutputSelection::Mode(OutputMode::Human);
    let text = if bare {
        Cli::command().render_help().to_string()
    } else {
        failure.diagnostic().to_string()
    };
    let _ = io::stderr().lock().write_all(text.as_bytes());
    ARGUMENT_ERROR_EXIT
}

fn rejection(entry: Option<&CommandEntry>, code: ErrorCode) -> i32 {
    let stages = entry.map_or_else(ExecutionMeta::not_executed, CommandEntry::stages);
    let outcome = Envelope::<(), ()>::error(
        entry.map(|entry| entry.canonical),
        code,
        Warnings::new(),
        stages,
    );
    emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::order::{ListData, ListOrdersInput};
    use nostr_sdk::prelude::Keys;
    use std::io::{Error, ErrorKind};

    /// A stream whose every write fails like a closed pipe.
    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(Error::from(ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(Error::from(ErrorKind::BrokenPipe))
        }
    }

    fn listed() -> ListOutcome {
        let filters = ListOrdersInput::parse(None, None, None).unwrap();
        let node = Keys::generate().public_key();
        let (data, warnings) = ListData::from_selected(&node, filters, &[]).unwrap();
        Envelope::ok(COMMAND, data, warnings, ExecutionMeta::read(None))
    }

    fn failed(code: ErrorCode) -> ListOutcome {
        Envelope::error(
            Some(COMMAND),
            code,
            Warnings::new(),
            ExecutionMeta::read(None),
        )
    }

    #[test]
    fn human_listing_is_written_once_with_the_outcome_exit() {
        let outcome = listed();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(print_list(&outcome, &mut out, &mut err), 0);
        assert!(err.is_empty());
        assert_eq!(out, render_list(outcome.data().unwrap()).into_bytes());
    }

    #[test]
    fn failed_human_write_exits_1_with_one_fixed_line() {
        let mut err = Vec::new();
        assert_eq!(print_list(&listed(), &mut Broken, &mut err), 1);
        assert_eq!(err, b"error: the order list could not be written\n");
    }

    #[test]
    fn human_error_writes_nothing_to_stdout() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = print_list(&failed(ErrorCode::ReadTimeout), &mut out, &mut err);
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert_eq!(
            String::from_utf8(err).unwrap(),
            format!("error: {}\n", ErrorCode::ReadTimeout.message())
        );
    }

    #[test]
    fn unwritable_stderr_never_changes_the_result() {
        assert_eq!(
            print_list(
                &failed(ErrorCode::ReadTimeout),
                &mut Vec::new(),
                &mut Broken
            ),
            1
        );
        assert_eq!(print_list(&listed(), &mut Broken, &mut Broken), 1);
        assert_eq!(print_list(&listed(), &mut Vec::new(), &mut Broken), 0);
        diagnose(&mut Broken, "verbose: ignored");
    }
}
