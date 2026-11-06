//! Dispatch of `dispute open` and `order rate`: a request on an order the user
//! holds, signed by its stored key, with no trade index. The same shape as the
//! trade steps: everything that can fail locally fails before the question,
//! the question is asked in human mode only, and exactly one request is
//! published. The shared steps are in [`super::write`].

use uuid::Uuid;

use super::commands::{RateArgs, StepArgs};
use super::invocation::OutputMode;
use super::step::warn;
use super::write::{confirmed, finish, needs_yes, refuse, resolve_base, Base};
use super::Cli;
use crate::commands::order::dispute::{dispute_open, DisputeOutcome};
use crate::commands::order::rate::{order_rate, RateOutcome};
use crate::msg::ExchangeLimits;
use crate::outcome::dispute::COMMAND as DISPUTE;
use crate::outcome::rate::{RateInput, COMMAND as RATE};
use crate::outcome::ErrorCode;
use crate::parse_human::{
    render_dispute, render_dispute_preview, render_rate, render_rate_preview,
};

const UNWRITABLE: &str = "error: the result could not be written";
const QUESTION: &str = "Send this request?";
const DISPUTE_ADVICE: &str = "The node may have opened the dispute; it answers a second request \
                              with silence. Check `order info --order <id>` before trying again.";
const RATE_ADVICE: &str = "The node may have recorded the rating; it answers a second one with \
                           silence. Do not send it again.";

/// Run `dispute open` and return its exit status.
pub(super) async fn dispute(cli: &Cli, args: &StepArgs, output: OutputMode) -> i32 {
    let outcome = dispute_outcome(cli, args, output).await;
    if output == OutputMode::Human {
        warn(&outcome);
    }
    finish(
        cli,
        output,
        &outcome,
        render_dispute,
        (UNWRITABLE, DISPUTE_ADVICE),
    )
}

/// Run `order rate` and return its exit status.
pub(super) async fn rate(cli: &Cli, args: &RateArgs, output: OutputMode) -> i32 {
    let outcome = rate_outcome(cli, args, output).await;
    // No warning applies to a rating today; a future one must not be lost.
    if output == OutputMode::Human {
        warn(&outcome);
    }
    finish(
        cli,
        output,
        &outcome,
        render_rate,
        (UNWRITABLE, RATE_ADVICE),
    )
}

async fn dispute_outcome(cli: &Cli, args: &StepArgs, output: OutputMode) -> DisputeOutcome {
    let (base, question) = match prepare(cli, args.order, args.yes, output, true).await {
        Ok(prepared) => prepared,
        Err(code) => return refuse(DISPUTE, code),
    };
    if let Some(trade) = question {
        if !confirmed(&render_dispute_preview(args.order, &trade), QUESTION) {
            return refuse(DISPUTE, ErrorCode::OrderDeclined);
        }
    }
    dispute_open(
        args.order,
        &base.config,
        &base.identity,
        &base.store,
        base.options,
        &ExchangeLimits::default(),
    )
    .await
}

async fn rate_outcome(cli: &Cli, args: &RateArgs, output: OutputMode) -> RateOutcome {
    if needs_yes(output, args.yes) {
        return refuse(RATE, ErrorCode::InvalidArguments);
    }
    let Ok(input) = RateInput::validate(args.order, args.rating) else {
        return refuse(RATE, ErrorCode::InvalidArguments);
    };
    // A rating writes nothing locally, so the store need not be writable.
    let (base, question) = match prepare(cli, args.order, args.yes, output, false).await {
        Ok(prepared) => prepared,
        Err(code) => return refuse(RATE, code),
    };
    if let Some(trade) = question {
        if !confirmed(&render_rate_preview(&input, &trade), QUESTION) {
            return refuse(RATE, ErrorCode::OrderDeclined);
        }
    }
    order_rate(
        &input,
        &base.config,
        &base.identity,
        &base.store,
        base.options,
        &ExchangeLimits::default(),
    )
    .await
}

/// Everything that can fail before the request: the JSON `--yes` rule,
/// configuration, identity and store (writable when the command records a
/// status). Without `--yes` the order's trade key is read for the preview:
/// `Some(key)` means a question must be asked.
async fn prepare(
    cli: &Cli,
    order: Uuid,
    yes: bool,
    output: OutputMode,
    writable: bool,
) -> Result<(Base, Option<String>), ErrorCode> {
    if needs_yes(output, yes) {
        return Err(ErrorCode::InvalidArguments);
    }
    let base = resolve_base(cli, writable).await?;
    if yes {
        return Ok((base, None));
    }
    match base.store.order_keys(order).await {
        Ok(Some(keys)) => Ok((base, Some(keys.public_key().to_hex()))),
        _ => Err(ErrorCode::OrderUnknown),
    }
}
