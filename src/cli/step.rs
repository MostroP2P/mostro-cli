//! Dispatch of `order fiat-sent`, `order release` and `order cancel`. The same
//! shape as the other write commands: everything that can fail locally fails
//! before the question is asked, the question is asked in human mode only, and
//! exactly one request is published. No trade is reserved: the order's stored
//! key signs. The shared steps are in [`super::write`].

use super::commands::StepArgs;
use super::invocation::OutputMode;
use super::migrated::diagnose;
use super::write::{confirmed, finish, needs_yes, refuse, resolve_base, Base};
use super::Cli;
use crate::commands::order::step::order_step;
use crate::msg::ExchangeLimits;
use crate::outcome::step::{StepData, StepInput, StepKind};
use crate::outcome::{Envelope, ErrorCode, WarningCode};
use crate::parse_human::{render_step, render_step_preview};

const UNWRITABLE: &str = "error: the result could not be written";
const INDEX_NOT_RECORDED: &str = "warning: the request was accepted but its new trade index \
                                  could not be recorded; the next order may reuse it.";
const STATUS_STALE: &str = "warning: the node accepted the request but the local order status \
                            could not be updated; `order info --order <id>` shows the real one.";

/// Run the step of `kind` and return the exit status.
pub(super) async fn run(cli: &Cli, kind: StepKind, args: &StepArgs, output: OutputMode) -> i32 {
    let outcome = match prepare(cli, kind, args, output).await {
        Ok((input, base)) => {
            order_step(
                &input,
                &base.config,
                &base.identity,
                &base.store,
                base.options,
                &ExchangeLimits::default(),
            )
            .await
        }
        Err(outcome) => outcome,
    };
    if output == OutputMode::Human {
        warn(&outcome);
    }
    finish(
        cli,
        output,
        &outcome,
        render_step,
        (UNWRITABLE, advice(kind)),
    )
}

/// The line that follows an uncertain outcome.
fn advice(kind: StepKind) -> &'static str {
    match kind {
        StepKind::FiatSent => {
            "The node may have recorded this. Check `order info --order <id>` before sending it again."
        }
        StepKind::Release => {
            "The node may have released the sats. Check `order info --order <id>` before releasing again."
        }
        StepKind::Cancel => {
            "The node may have canceled or started a cancel. Check `order info --order <id>` before canceling again."
        }
    }
}

/// The warnings a human cannot read in a document: fixed lines on stderr.
fn warn(outcome: &Envelope<StepData, ()>) {
    let mut err = std::io::stderr().lock();
    if outcome
        .warnings()
        .contains(WarningCode::TradeIndexNotRecorded)
    {
        diagnose(&mut err, INDEX_NOT_RECORDED);
    }
    if outcome.warnings().contains(WarningCode::LocalStatusStale) {
        diagnose(&mut err, STATUS_STALE);
    }
}

/// Everything that can fail before the request: the confirmation rule,
/// configuration, identity and a writable store, the order and the question.
/// The error side is the finished outcome.
async fn prepare(
    cli: &Cli,
    kind: StepKind,
    args: &StepArgs,
    output: OutputMode,
) -> Result<(StepInput, Base), Envelope<StepData, ()>> {
    let command = kind.command();
    if needs_yes(output, args.yes) {
        return Err(refuse(command, ErrorCode::InvalidArguments));
    }
    let input = StepInput::new(kind, args.order);
    let base = resolve_base(cli, true)
        .await
        .map_err(|code| refuse(command, code))?;
    if !args.yes {
        let trade = match base.store.order_keys(input.order()).await {
            Ok(Some(keys)) => keys.public_key().to_hex(),
            _ => return Err(refuse(command, ErrorCode::OrderUnknown)),
        };
        let preview = render_step_preview(&input, &trade);
        if !confirmed(&preview, "Send this request?") {
            return Err(refuse(command, ErrorCode::OrderDeclined));
        }
    }
    Ok((input, base))
}
