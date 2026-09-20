use crate::cli::dm_to_user::execute_dm_to_user;
use crate::cli::get_dm_user::execute_get_dm_user_labelled;
use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{print_info_line, print_key_value, print_section_header};
use anyhow::Result;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

/// Read, and optionally write to, the dispute conversation with the solver.
///
/// A dispute has its own channel. The peer chat is derived from the two trade
/// keys and nobody else can read it, not even the instance; the dispute chat
/// is derived from your trade key and the solver's pubkey, and there the
/// solver is a legitimate writer. Both use the same envelope, so the only
/// thing standing between a user and their solver was knowing which pubkey to
/// derive from.
///
/// `getdm` cannot show these messages: it filters on events tagged with the
/// user's own pubkey, while the dispute chat is tagged to the conversation
/// key. Pointing users at `getdm` after opening a dispute, as the README did,
/// leaves them waiting for a message they will never see.
pub async fn execute_dispute_chat(
    order_id: Uuid,
    since: &i64,
    message: Option<&str>,
    ctx: &Context,
) -> Result<()> {
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load order {order_id}: {e}"))?;

    let Some(solver) = order.solver_pubkey.as_ref() else {
        print_section_header("⚖️ Dispute Chat");
        print_key_value("📋", "Order ID", &order_id.to_string());
        print_info_line(
            "💡",
            "No solver known for this order yet. The pubkey arrives with the \
             admin-took-dispute message; run `getdm` once to receive it.",
        );
        return Ok(());
    };
    let solver_pubkey = PublicKey::from_str(solver)
        .map_err(|e| anyhow::anyhow!("Stored solver pubkey is not valid: {e}"))?;

    if let Some(text) = message {
        execute_dm_to_user(solver_pubkey, &ctx.client, &order_id, text, &ctx.pool).await?;
    }

    execute_get_dm_user_labelled(solver_pubkey, order_id, since, "Solver", ctx).await
}
