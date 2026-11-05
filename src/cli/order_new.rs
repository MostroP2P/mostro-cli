//! Dispatch of `order new`: the first migrated command that writes. It checks
//! everything that can fail locally before it asks for confirmation, asks only
//! in human mode, and publishes exactly one request. No legacy context, logger
//! from user input or RPC is involved.

use std::env;
use std::io::{self, BufRead, Write};

use mostro_core::order::Kind;

use super::commands::NewArgs;
use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose, request_options};
use super::Cli;
use crate::commands::order::new::{order_new, unsent, NewOptions, NewOutcome, COMMAND};
use crate::identity::{load_identity_keys, IdentityError};
use crate::msg::{ExchangeLimits, PublicReadConfig};
use crate::outcome::new::{InvalidOrder, OrderNewInput, OrderRequest};
use crate::outcome::{ErrorCode, Status, RUNTIME_ERROR_EXIT};
use crate::parse_human::{render_new, render_preview};
use crate::parse_machine::emit;
use crate::store::{Store, StoreError, Trade};

/// Run `order new` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &NewArgs, output: OutputMode) -> i32 {
    let outcome = outcome(cli, args, output).await;
    let exit = match output {
        OutputMode::Json => emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
        OutputMode::Human => {
            print_new(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock())
        }
    };
    if cli.verbose {
        diagnose(&mut io::stderr().lock(), &summary(&outcome));
    }
    exit
}

/// Everything the request needs, all checked before anything is published.
struct Prepared {
    input: OrderNewInput,
    config: PublicReadConfig,
    options: NewOptions,
    identity: nostr_sdk::prelude::Keys,
    store: Store,
    trade: Trade,
}

async fn outcome(cli: &Cli, args: &NewArgs, output: OutputMode) -> NewOutcome {
    // JSON never prompts: without --yes it does nothing at all.
    if output == OutputMode::Json && !args.yes {
        return refuse(ErrorCode::InvalidArguments);
    }
    let prepared = match prepare(cli, args).await {
        Ok(prepared) => prepared,
        Err(code) => return refuse(code),
    };
    if !args.yes && !confirmed_by_user(&prepared) {
        return refuse(ErrorCode::OrderDeclined);
    }
    order_new(
        &prepared.input,
        &prepared.config,
        &prepared.identity,
        &prepared.trade,
        &prepared.store,
        prepared.options,
        &ExchangeLimits::default(),
    )
    .await
}

fn refuse(code: ErrorCode) -> NewOutcome {
    unsent(code, crate::outcome::Warnings::new())
}

/// Validate, then resolve configuration, identity and store in order of cost.
async fn prepare(cli: &Cli, args: &NewArgs) -> Result<Prepared, ErrorCode> {
    let getenv = |name: &str| env::var(name).ok();
    let input = validate(args).map_err(|_| ErrorCode::InvalidArguments)?;
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
    Ok(Prepared {
        input,
        config,
        options: NewOptions {
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

fn validate(args: &NewArgs) -> Result<OrderNewInput, InvalidOrder> {
    OrderNewInput::validate(OrderRequest {
        side: if args.side.eq_ignore_ascii_case("buy") {
            Kind::Buy
        } else {
            Kind::Sell
        },
        currency: args.currency.clone(),
        fiat_amount: args.fiat_amount,
        fiat_amount_max: args.fiat_amount_max,
        payment_method: args.payment_method.clone(),
        sats: args.sats,
        premium: args.premium,
        invoice: args.invoice.clone(),
        expiration_days: args.expiration_days,
    })
}

/// Show the preview on stdout and ask on stderr; anything but blank or `y` (and
/// a closed stdin) declines.
fn confirmed_by_user(prepared: &Prepared) -> bool {
    let preview = render_preview(
        &prepared.input,
        &prepared.trade.index.to_string(),
        &prepared.trade.keys.public_key().to_hex(),
    );
    let mut out = io::stdout().lock();
    if out
        .write_all(preview.as_bytes())
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let mut err = io::stderr().lock();
    let _ = write!(err, "Create this order? [Y/n] ");
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

/// The only `--verbose` line of `order new`: codes and the status, never
/// the order, key, relay, invoice or node text.
fn summary(outcome: &NewOutcome) -> String {
    let result = outcome
        .error_code()
        .map_or_else(|| "ok".to_string(), code_name);
    format!("verbose: {COMMAND} {result}")
}

/// The human result on `out`; errors, and the guidance of an uncertain
/// outcome, as fixed lines on `err`. Rendered completely before the first byte.
fn print_new(outcome: &NewOutcome, out: &mut impl Write, err: &mut impl Write) -> i32 {
    if let Some(code) = outcome.error_code() {
        let line = match outcome.reason() {
            Some(reason) => format!("error: {} (reason: {reason})", code.message()),
            None => format!("error: {}", code.message()),
        };
        diagnose(err, &line);
        if outcome.status() == Status::Uncertain {
            diagnose(
                err,
                "The node may hold this order. Check `order list` before trying again.",
            );
        }
    }
    let Some(data) = outcome.data() else {
        return outcome.exit_code();
    };
    match out
        .write_all(render_new(data).as_bytes())
        .and_then(|()| out.flush())
    {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, "error: the new order could not be written");
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
}
