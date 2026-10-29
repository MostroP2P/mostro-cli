//! Dispatch of the commands that run without the legacy context: offline
//! discovery and the public listing. They need no identity, database, logger
//! from user input or RPC, and they own what they write to stdout and stderr.

use std::env;
use std::io::{self, Write};

use chrono::Utc;
use uuid::Uuid;

use super::invocation::OutputMode;
use super::schema::SCHEMA_COMMAND;
use super::{offline_discovery, Cli, Commands, DiscoveryData, OrderCommands};
use crate::commands::order::info::{order_info, InfoOptions, InfoOutcome, COMMAND as INFO};
use crate::commands::order::list::{list_orders, ListOutcome, COMMAND};
use crate::identity::{load_identity_keys, IdentityError};
use crate::msg::{ExchangeLimits, PublicReadConfig, ReadLimits};
use crate::outcome::info::OrderInfoInput;
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Publication, Warnings, RUNTIME_ERROR_EXIT,
};
use crate::parse_human::{render_info, render_list};
use crate::parse_machine::emit;

/// Run `cli.command` if it is a migrated command and return its exit status;
/// `None` for every other command.
pub(super) async fn run(cli: &Cli, output: OutputMode) -> Option<i32> {
    // Offline discovery needs no configuration, identity, logger or network.
    if let Some(Commands::Schema) = &cli.command {
        let data = offline_discovery();
        return Some(match output {
            OutputMode::Json => {
                let outcome = Envelope::<DiscoveryData, ()>::ok(
                    SCHEMA_COMMAND,
                    data,
                    Warnings::new(),
                    ExecutionMeta::read(None),
                );
                emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
            }
            OutputMode::Human => {
                print_discovery(&data, &mut io::stdout().lock(), &mut io::stderr().lock())
            }
        });
    }

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
        return Some(exit);
    }

    // The node's record of the user's own orders: one signed request, an
    // existing identity read-only, no environment mutation or legacy context.
    if let Some(Commands::Order {
        command: OrderCommands::Info { orders },
    }) = &cli.command
    {
        let outcome = info_outcome(cli, orders).await;
        let exit = match output {
            OutputMode::Json => emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
            OutputMode::Human => {
                print_info(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
            }
        };
        if cli.verbose {
            diagnose(&mut io::stderr().lock(), &info_summary(&outcome));
        }
        return Some(exit);
    }
    None
}

/// Resolve configuration and identity in order of cost and ask the node.
/// Everything that can fail locally fails before anything is published.
async fn info_outcome(cli: &Cli, orders: &[Uuid]) -> InfoOutcome {
    let getenv = |name: &str| env::var(name).ok();
    let Some(requested) = OrderInfoInput::new(orders.iter().copied()) else {
        return unsent(ErrorCode::InvalidArguments);
    };
    let config = match PublicReadConfig::resolve(
        cli.mostro_pubkey.as_deref(),
        cli.relays.as_deref(),
        getenv,
    ) {
        Ok(config) => config,
        Err(code) => return unsent(code),
    };
    let options = match request_options(cli.pow.as_deref(), cli.transport.as_deref(), getenv) {
        Ok(options) => options,
        Err(code) => return unsent(code),
    };
    let identity = match load_identity_keys().await {
        Ok(keys) => keys,
        Err(IdentityError::Missing) => return unsent(ErrorCode::IdentityMissing),
        Err(IdentityError::Unreadable) => return unsent(ErrorCode::IdentityUnreadable),
    };
    order_info(
        &requested,
        &config,
        &identity,
        options,
        &ExchangeLimits::default(),
    )
    .await
}

/// A failure before anything was published.
fn unsent(code: ErrorCode) -> InfoOutcome {
    Envelope::error(
        Some(INFO),
        code,
        Warnings::new(),
        ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown),
    )
}

/// The work to mine and whether to check the node's protocol: `--pow` over
/// `POW` (0-255, default 0) and `--transport` over `TRANSPORT`, of which only
/// `nip44` exists. An explicit transport skips the protocol check.
fn request_options(
    pow: Option<&str>,
    transport: Option<&str>,
    getenv: impl Fn(&str) -> Option<String>,
) -> Result<InfoOptions, ErrorCode> {
    let given = |cli: Option<&str>, name: &str| {
        cli.map(str::to_owned)
            .or_else(|| getenv(name))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let pow = match given(pow, "POW") {
        None => 0,
        Some(text) => text
            .parse::<u8>()
            .map_err(|_| ErrorCode::ConfigurationInvalid)?,
    };
    let transport = given(transport, "TRANSPORT");
    if transport.as_deref().is_some_and(|value| value != "nip44") {
        return Err(ErrorCode::ConfigurationInvalid);
    }
    Ok(InfoOptions {
        pow,
        check_protocol: transport.is_none(),
    })
}

/// The only `--verbose` line of `order info`: codes and counts, never ids,
/// relay, URL or node text.
fn info_summary(outcome: &InfoOutcome) -> String {
    let result = outcome
        .error_code()
        .map_or_else(|| "ok".to_string(), code_name);
    match outcome.data() {
        Some(data) => format!(
            "verbose: {INFO} {result}; {} returned, {} not returned",
            data.orders().len(),
            data.not_returned().len()
        ),
        None => format!("verbose: {INFO} {result}"),
    }
}

/// The human result on `out`, or a fixed error line on `err` (with the node's
/// reason when it refused). Rendered completely before the first byte is written.
fn print_info(outcome: &InfoOutcome, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let Some(data) = outcome.data() else {
        let message = outcome
            .error_code()
            .unwrap_or(ErrorCode::InternalError)
            .message();
        let line = match outcome.reason() {
            Some(reason) => format!("error: {message} (reason: {reason})"),
            None => format!("error: {message}"),
        };
        diagnose(err, &line);
        return outcome.exit_code();
    };
    let text = render_info(data);
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, "error: the order information could not be written");
            RUNTIME_ERROR_EXIT
        }
    }
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
pub(super) fn diagnose(err: &mut impl Write, line: &str) {
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

/// The discovery document as pretty JSON on `out`. It is serialized completely
/// before the first byte is written; a failure exits 1 with one fixed line.
fn print_discovery(data: &DiscoveryData, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let Ok(mut text) = serde_json::to_string_pretty(data) else {
        diagnose(
            err,
            &format!("error: {}", ErrorCode::InternalError.message()),
        );
        return RUNTIME_ERROR_EXIT;
    };
    text.push('\n');
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => 0,
        Err(_) => {
            diagnose(err, "error: the discovery document could not be written");
            RUNTIME_ERROR_EXIT
        }
    }
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

    fn discovered() -> DiscoveryData {
        offline_discovery()
    }

    #[test]
    fn discovery_prints_one_pretty_document_with_the_schema_command() {
        let data = discovered();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(print_discovery(&data, &mut out, &mut err), 0);
        assert!(err.is_empty());
        let text = String::from_utf8(out).unwrap();
        assert!(text.ends_with("}\n") && text.contains("\n  \"commands\""));
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, serde_json::to_value(&data).unwrap());
        let names: Vec<&str> = parsed["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|command| command["name"].as_str())
            .collect();
        assert!(names.contains(&"schema") && names.contains(&"order.list"));
    }

    #[test]
    fn failed_discovery_write_exits_1_with_one_fixed_line() {
        let mut err = Vec::new();
        assert_eq!(print_discovery(&discovered(), &mut Broken, &mut err), 1);
        assert_eq!(err, b"error: the discovery document could not be written\n");
        assert_eq!(
            print_discovery(&discovered(), &mut Vec::new(), &mut Broken),
            0
        );
    }
}
