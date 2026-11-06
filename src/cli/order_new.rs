//! Dispatch of `order new`: the first migrated command that writes. It checks
//! everything that can fail locally before it asks for confirmation, asks only
//! in human mode, and publishes exactly one request. The steps every write
//! command shares are in [`super::write`]. No legacy context, logger from user
//! input or RPC is involved.

use mostro_core::order::Kind;

use super::commands::NewArgs;
use super::invocation::OutputMode;
use super::stdin;
use super::write::{
    confirmed, finish, needs_yes, preview_trade, refuse, refuse_invoice, resolve_base,
};
use super::Cli;
use crate::commands::order::new::{order_new, NewOutcome, COMMAND};
use crate::msg::ExchangeLimits;
use crate::outcome::new::{InvalidOrder, OrderNewInput, OrderRequest};
use crate::outcome::ErrorCode;
use crate::parse_human::{render_new, render_preview};

const UNWRITABLE: &str = "error: the new order could not be written";
const ADVICE: &str = "The node may hold this order. Check `order list` before trying again.";

/// Run `order new` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &NewArgs, output: OutputMode) -> i32 {
    let outcome = outcome(cli, args, output).await;
    finish(cli, output, &outcome, render_new, (UNWRITABLE, ADVICE))
}

async fn outcome(cli: &Cli, args: &NewArgs, output: OutputMode) -> NewOutcome {
    if needs_yes(output, args.yes) {
        return refuse(COMMAND, ErrorCode::InvalidArguments);
    }
    // `--invoice -`: read it now, before anything else can fail or ask.
    let invoice = match stdin::invoice(args.invoice.clone(), args.yes) {
        Ok(invoice) => invoice,
        Err(refusal) => return refusal.outcome(COMMAND),
    };
    let input = match validate(args, invoice) {
        Ok(input) => input,
        Err(InvalidOrder::Invoice) => return refuse_invoice(COMMAND),
        Err(_) => return refuse(COMMAND, ErrorCode::InvalidArguments),
    };
    let base = match resolve_base(cli, true).await {
        Ok(base) => base,
        Err(code) => return refuse(COMMAND, code),
    };
    if !args.yes {
        let trade = match preview_trade(&base.store).await {
            Ok(trade) => trade,
            Err(code) => return refuse(COMMAND, code),
        };
        let preview = render_preview(
            &input,
            &trade.index.to_string(),
            &trade.keys.public_key().to_hex(),
        );
        if !confirmed(&preview, "Create this order?") {
            return refuse(COMMAND, ErrorCode::OrderDeclined);
        }
    }
    order_new(
        &input,
        &base.config,
        &base.identity,
        &base.store,
        base.options,
        &ExchangeLimits::default(),
    )
    .await
}

/// `invoice` is `--invoice` after `-` was resolved from stdin.
fn validate(args: &NewArgs, invoice: Option<String>) -> Result<OrderNewInput, InvalidOrder> {
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
        invoice,
        expiration_days: args.expiration_days,
    })
}
