//! The part of the dispatch that every migrated write command shares: resolve
//! configuration, identity and store in order of cost, ask for confirmation in
//! human mode only, and present the outcome. A command supplies its own input
//! validation, preview, operation and renderer. Everything that can fail
//! locally fails before anything is published.

use std::env;
use std::io::{self, BufRead, Write};
use std::time::Duration;

use nostr_sdk::prelude::Keys;
use serde::Serialize;

use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose, error_line};
use super::Cli;
use crate::commands::order::info::InfoOptions;
use crate::commands::order::WriteOptions;
use crate::identity::{load_identity_keys, IdentityError};
use crate::msg::{ExchangeLimits, PublicReadConfig};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, Status,
    Warnings, INVALID_INVOICE, RUNTIME_ERROR_EXIT,
};
use crate::parse_machine::emit;
use crate::settings::{configured_config, env_text, setting};
use crate::store::{Store, StoreError, Trade};

/// Configuration, identity and store, all checked; no trade is reserved (the
/// operation reserves its own, right before publishing).
pub(super) struct Base {
    pub config: PublicReadConfig,
    pub options: WriteOptions,
    pub identity: Keys,
    pub store: Store,
}

/// Configuration, identity and store of a command that sends the node nothing.
pub(super) struct Reader {
    pub config: PublicReadConfig,
    pub identity: Keys,
    pub store: Store,
}

/// The lookup of the node settings that a root option did not give: the
/// environment variable when it is set (even empty, or not Unicode, which
/// counts as empty), else the config file of the data directory, read once
/// here. A file it refuses is `configuration_invalid`, with one `hint:` line
/// on stderr naming the file and the line, never the content.
pub(super) fn node_lookup() -> Result<impl Fn(&str) -> Option<String>, ErrorCode> {
    match configured_config() {
        Ok(file) => {
            Ok(move |name: &str| setting(file.as_ref(), name, |name| env_text(env::var_os(name))))
        }
        Err(problem) => {
            diagnose(&mut io::stderr().lock(), &format!("hint: {problem}"));
            Err(ErrorCode::ConfigurationInvalid)
        }
    }
}

/// Resolve configuration, then the existing identity (read-only), then the
/// store, which must accept a write when `writable`. The work and transport
/// settings concern only what is sent to the node, so neither a read nor the
/// peer chat (to relays only, mining nothing) looks at them.
pub(super) async fn resolve_reader(cli: &Cli, writable: bool) -> Result<Reader, ErrorCode> {
    let lookup = node_lookup()?;
    let config =
        PublicReadConfig::resolve(cli.mostro_pubkey.as_deref(), cli.relays.as_deref(), lookup)?;
    reader_with(config, writable).await
}

/// [`resolve_reader`] and then the work and transport settings, which a
/// request needs. The order of the checks is that of a request: the config
/// file, the configuration, then the settings, then identity and store.
pub(super) async fn resolve_base(cli: &Cli, writable: bool) -> Result<Base, ErrorCode> {
    let lookup = node_lookup()?;
    let config =
        PublicReadConfig::resolve(cli.mostro_pubkey.as_deref(), cli.relays.as_deref(), &lookup)?;
    let request = request_options(cli.pow.as_deref(), cli.transport.as_deref(), &lookup)?;
    let reader = reader_with(config, writable).await?;
    Ok(Base {
        config: reader.config,
        options: WriteOptions {
            pow: request.pow,
            check_protocol: request.check_protocol,
        },
        identity: reader.identity,
        store: reader.store,
    })
}

/// The existing identity (read-only) and the store of a resolved
/// configuration; the store must accept a write when `writable`.
async fn reader_with(config: PublicReadConfig, writable: bool) -> Result<Reader, ErrorCode> {
    let identity = load_identity_keys().await.map_err(|error| match error {
        IdentityError::Missing => ErrorCode::IdentityMissing,
        IdentityError::Unreadable => ErrorCode::IdentityUnreadable,
    })?;
    let store = Store::configured().map_err(store_code)?;
    if writable {
        store.ensure_writable().await.map_err(store_code)?;
    }
    Ok(Reader {
        config,
        identity,
        store,
    })
}

/// The least work to mine and whether to check the node's protocol: `--pow`
/// over `POW` (0-255, default 0; the exchange raises it to what the node
/// advertises, up to `AUTO_POW_CEILING`) and `--transport` over `TRANSPORT`,
/// of which only `nip44` exists. An explicit transport skips the protocol check.
/// `getenv` answers for the variables: the environment, then the config file.
pub(super) fn request_options(
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

/// The key and index the human preview shows: a peek that writes nothing,
/// taken only when a question is asked. The request reserves its own trade
/// after the answer; it is this one unless another command reserved one
/// meanwhile, and the result reports the one actually used.
pub(super) async fn preview_trade(store: &Store) -> Result<Trade, ErrorCode> {
    store.next_trade().await.map_err(store_code)
}

fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable => ErrorCode::StoreUnwritable,
        StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}

/// The budgets of a request that waits for the node's reply: the reply
/// deadline of `--reply-timeout`, the other budgets as they are.
pub(super) fn exchange_limits(cli: &Cli) -> ExchangeLimits {
    ExchangeLimits {
        reply: Duration::from_secs(cli.reply_timeout),
        ..ExchangeLimits::default()
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

/// An `invalid_arguments` caused by the invoice value itself, before anything
/// was published or written: it carries the reason `invalid_invoice`.
pub(super) fn refuse_invoice<D>(command: &'static str) -> Envelope<D, ()> {
    Envelope::invalid_input(
        Some(command),
        INVALID_INVOICE,
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
    if outcome.error_code().is_some() {
        diagnose(err, &error_line(outcome));
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
    use crate::settings::ConfigFile;

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
    fn request_options_takes_pow_and_transport_through_the_file_lookup() {
        let file = ConfigFile::parse("POW=3\nTRANSPORT=nip44\n").unwrap();
        let from_file = |name: &str| setting(Some(&file), name, |_| None);
        let options = request_options(None, None, from_file).unwrap();
        assert_eq!((options.pow, options.check_protocol), (3, false));
        // A set but empty variable is 0 over the file's value.
        let empty_pow = |name: &str| setting(Some(&file), name, |n| (n == "POW").then(String::new));
        assert_eq!(request_options(None, None, empty_pow).unwrap().pow, 0);
        // A flag beats both.
        let options = request_options(Some("7"), Some("nip44"), empty_pow).unwrap();
        assert_eq!((options.pow, options.check_protocol), (7, false));
        let detect = request_options(None, None, |_| None).unwrap();
        assert_eq!((detect.pow, detect.check_protocol), (0, true));
    }

    #[test]
    fn json_needs_yes_and_human_never_does() {
        assert!(needs_yes(OutputMode::Json, false));
        assert!(!needs_yes(OutputMode::Json, true));
        assert!(!needs_yes(OutputMode::Human, false));
    }
}
