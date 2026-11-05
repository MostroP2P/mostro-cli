//! Dispatch of `order add-invoice` and `bond add-invoice`. The same shape as
//! the other write commands: everything that can fail locally fails before the
//! question is asked, the question is asked in human mode only, and exactly one
//! request is published. Neither command reserves a trade: the order's stored
//! key signs. The shared steps are in [`super::write`].

use super::commands::{AddBondInvoiceArgs, AddInvoiceArgs};
use super::invocation::OutputMode;
use super::write::{confirmed, finish, needs_yes, refuse, resolve_base, Base};
use super::Cli;
use crate::commands::order::invoice::{bond_add_invoice, order_add_invoice};
use crate::msg::ExchangeLimits;
use crate::outcome::invoice::{InvoiceInput, InvoiceKind, InvoiceRequest};
use crate::outcome::{Envelope, ErrorCode};
use crate::parse_human::{render_bond_invoice, render_invoice_preview, render_order_invoice};
use crate::store::StoreError;

const UNWRITABLE: &str = "error: the invoice result could not be written";
const ORDER_ADVICE: &str =
    "The node may hold this invoice. Check `order info --order <id>` before sending another.";
const BOND_ADVICE: &str = "The node may hold this invoice; an older node never confirms it. \
                           Do not send another before the payout arrives.";

/// Run `order add-invoice` and return its exit status.
pub(super) async fn order(cli: &Cli, args: &AddInvoiceArgs, output: OutputMode) -> i32 {
    let request = InvoiceRequest {
        kind: InvoiceKind::Order,
        order: args.order,
        invoice: args.invoice.clone(),
    };
    let outcome = match prepare(cli, request, args.yes, output).await {
        Ok((input, base)) => {
            order_add_invoice(
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
    finish(
        cli,
        output,
        &outcome,
        render_order_invoice,
        (UNWRITABLE, ORDER_ADVICE),
    )
}

/// Run `bond add-invoice` and return its exit status.
pub(super) async fn bond(cli: &Cli, args: &AddBondInvoiceArgs, output: OutputMode) -> i32 {
    let request = InvoiceRequest {
        kind: InvoiceKind::Bond,
        order: args.order,
        invoice: args.invoice.clone(),
    };
    let outcome = match prepare(cli, request, args.yes, output).await {
        Ok((input, base)) => {
            bond_add_invoice(
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
    finish(
        cli,
        output,
        &outcome,
        render_bond_invoice,
        (UNWRITABLE, BOND_ADVICE),
    )
}

/// Everything that can fail before the request: the confirmation rule, the
/// input, configuration, identity and store (writable only for the order
/// invoice, which updates the local status), the order and the question. The
/// error side is the finished outcome.
async fn prepare<D>(
    cli: &Cli,
    request: InvoiceRequest,
    yes: bool,
    output: OutputMode,
) -> Result<(InvoiceInput, Base), Envelope<D, ()>> {
    let kind = request.kind;
    let command = kind.command();
    if needs_yes(output, yes) {
        return Err(refuse(command, ErrorCode::InvalidArguments));
    }
    let Ok(input) = InvoiceInput::validate(request) else {
        return Err(refuse(command, ErrorCode::InvalidArguments));
    };
    let base = resolve_base(cli, kind == InvoiceKind::Order)
        .await
        .map_err(|code| refuse(command, code))?;
    if !yes {
        let trade = match base.store.order_keys(input.order()).await {
            Ok(Some(keys)) => keys.public_key().to_hex(),
            // The same codes as the send path: a broken store is not an unknown order.
            Err(StoreError::Unwritable) => return Err(refuse(command, ErrorCode::StoreUnwritable)),
            _ => return Err(refuse(command, ErrorCode::OrderUnknown)),
        };
        let preview = render_invoice_preview(&input, &trade);
        if !confirmed(&preview, "Send this invoice?") {
            return Err(refuse(command, ErrorCode::OrderDeclined));
        }
    }
    Ok((input, base))
}
