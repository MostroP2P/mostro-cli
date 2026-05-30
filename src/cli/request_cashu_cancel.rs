use anyhow::Result;
use uuid::Uuid;

use crate::cli::Context;
use crate::db::Order;
use crate::util::messaging::{derive_shared_keys, send_admin_chat_message_via_shared_key};

pub async fn execute_request_cashu_cancel(
    order_id: &Uuid,
    buyer_pubkey: &str,
    ctx: &Context,
) -> Result<()> {
    let buyer_nostr_pk = nostr_sdk::PublicKey::parse(buyer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid buyer pubkey: {}", e))?;

    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Order {} not found: {}", order_id, e))?;

    let token_str = order
        .cashu_escrow_token
        .ok_or_else(|| anyhow::anyhow!("No Cashu escrow token found for order {}", order_id))?;

    let shared_keys = derive_shared_keys(Some(&ctx.trade_keys), Some(&buyer_nostr_pk))
        .ok_or_else(|| anyhow::anyhow!("Failed to derive shared key with buyer"))?;

    let message = format!("CANCEL:{}", token_str);

    send_admin_chat_message_via_shared_key(
        &ctx.client,
        &ctx.trade_keys,
        &shared_keys,
        &message,
    )
    .await?;

    println!(
        "Cancel request sent to buyer {}.",
        &buyer_pubkey[..16]
    );

    Ok(())
}
