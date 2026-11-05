//! Dispatch of `order new`: the first migrated command that writes. It checks
//! everything that can fail locally before it asks for confirmation, asks only
//! in human mode, and publishes exactly one request. The steps every write
//! command shares are in [`super::write`]. No legacy context, logger from user
//! input or RPC is involved.

use mostro_core::order::Kind;

use super::commands::NewArgs;
use super::invocation::OutputMode;
use super::write::{confirmed, finish, needs_yes, refuse, resolve};
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
    let Ok(input) = validate(args) else {
        return refuse(COMMAND, ErrorCode::InvalidArguments);
    };
    let resolved = match resolve(cli).await {
        Ok(resolved) => resolved,
        Err(code) => return refuse(COMMAND, code),
    };
    if !args.yes {
        let preview = render_preview(
            &input,
            &resolved.trade.index.to_string(),
            &resolved.trade.keys.public_key().to_hex(),
        );
        if !confirmed(&preview, "Create this order?") {
            return refuse(COMMAND, ErrorCode::OrderDeclined);
        }
    }
    order_new(
        &input,
        &resolved.config,
        &resolved.identity,
        &resolved.trade,
        &resolved.store,
        resolved.options,
        &ExchangeLimits::default(),
    )
    .await
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
