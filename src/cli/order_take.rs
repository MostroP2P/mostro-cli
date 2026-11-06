//! Dispatch of `order take-sell` and `order take-buy`. The same shape as
//! `order new`: everything that can fail locally fails before the question is
//! asked, the question is asked in human mode only, and exactly one request is
//! published. The shared steps are in [`super::write`].

use super::commands::{TakeBuyArgs, TakeSellArgs};
use super::invocation::OutputMode;
use super::stdin;
use super::write::{
    confirmed, finish, needs_yes, preview_trade, refuse, refuse_invoice, resolve_base,
};
use super::Cli;
use crate::commands::order::take::{order_take, TakeOutcome};
use crate::msg::ExchangeLimits;
use crate::outcome::take::{InvalidTake, OrderTakeInput, TakeRequest, TakeSide};
use crate::outcome::ErrorCode;
use crate::parse_human::{render_take_preview, render_taken};

const UNWRITABLE: &str = "error: the taken order could not be written";
const ADVICE: &str =
    "The node may hold this take. Check `order info --order <id>` before trying again.";

/// Run `order take-sell` and return its exit status.
pub(super) async fn sell(cli: &Cli, args: &TakeSellArgs, output: OutputMode) -> i32 {
    let request = TakeRequest {
        side: TakeSide::Sell,
        order: args.order,
        amount: args.amount,
        invoice: args.invoice.clone(),
    };
    run(cli, request, args.yes, output).await
}

/// Run `order take-buy` and return its exit status.
pub(super) async fn buy(cli: &Cli, args: &TakeBuyArgs, output: OutputMode) -> i32 {
    let request = TakeRequest {
        side: TakeSide::Buy,
        order: args.order,
        amount: args.amount,
        invoice: None,
    };
    run(cli, request, args.yes, output).await
}

async fn run(cli: &Cli, request: TakeRequest, yes: bool, output: OutputMode) -> i32 {
    let outcome = outcome(cli, request, yes, output).await;
    finish(cli, output, &outcome, render_taken, (UNWRITABLE, ADVICE))
}

async fn outcome(cli: &Cli, request: TakeRequest, yes: bool, output: OutputMode) -> TakeOutcome {
    let side = request.side;
    let command = side.command();
    if needs_yes(output, yes) {
        return refuse(command, ErrorCode::InvalidArguments);
    }
    // `--invoice -`: read it now, before anything else can fail or ask.
    let invoice = match stdin::invoice(request.invoice, yes) {
        Ok(invoice) => invoice,
        Err(refusal) => return refusal.outcome(command),
    };
    let request = TakeRequest { invoice, ..request };
    let input = match OrderTakeInput::validate(request) {
        Ok(input) => input,
        Err(InvalidTake::Invoice) => return refuse_invoice(command),
        Err(_) => return refuse(command, ErrorCode::InvalidArguments),
    };
    let base = match resolve_base(cli, true).await {
        Ok(base) => base,
        Err(code) => return refuse(command, code),
    };
    if !yes {
        let trade = match preview_trade(&base.store).await {
            Ok(trade) => trade,
            Err(code) => return refuse(command, code),
        };
        let preview = render_take_preview(
            &input,
            &trade.index.to_string(),
            &trade.keys.public_key().to_hex(),
        );
        if !confirmed(&preview, "Take this order?") {
            return refuse(command, ErrorCode::OrderDeclined);
        }
    }
    order_take(
        &input,
        &base.config,
        &base.identity,
        &base.store,
        base.options,
        &ExchangeLimits::default(),
    )
    .await
}
