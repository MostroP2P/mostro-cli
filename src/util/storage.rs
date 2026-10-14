use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::cli::send_msg::execute_send_msg;
use crate::cli::{Commands, Context};
use crate::db::{Order, User};
use crate::util::messaging::get_admin_keys;
use crate::util::send_dm;

/// Saves `order` with the keys of the trade that made or took it, and records
/// `trade_index` as the last one used.
///
/// Both writes are what the next command relies on: a missing row leaves the
/// trade unusable from this client, and a stale index makes the next order
/// reuse a key the node has already seen. So either failure is an error,
/// never just a printed line.
pub async fn save_order(
    order: SmallOrder,
    trade_keys: &Keys,
    request_id: u64,
    trade_index: i64,
    pool: &SqlitePool,
) -> Result<()> {
    let order = Order::new(pool, order, trade_keys, Some(request_id as i64))
        .await
        .map_err(|e| anyhow::anyhow!("could not save the order locally: {e}"))?;
    if let Some(order_id) = order.id {
        println!("Order {} created", order_id);
    } else {
        println!("Warning: The newly created order has no ID.");
    }

    let mut user = User::get(pool).await.map_err(|e| {
        anyhow::anyhow!("could not load the user to record trade index {trade_index}: {e}")
    })?;
    user.set_last_trade_index(trade_index);
    user.save(pool)
        .await
        .map_err(|e| anyhow::anyhow!("could not record trade index {trade_index}: {e}"))?;
    Ok(())
}

pub async fn run_simple_order_msg(
    command: Commands,
    order_id: Option<Uuid>,
    ctx: &Context,
) -> Result<()> {
    execute_send_msg(command, order_id, ctx, None).await
}

pub async fn admin_send_dm(ctx: &Context, msg: String) -> Result<()> {
    // Get admin keys
    let admin_keys = get_admin_keys(ctx)?;
    // Admin identity binds via the seal signer and the rumor author. The
    // admin role doesn't rotate per-trade keys, so the same `admin_keys`
    // signs both layers (full-privacy-style wrap in mostro-core 0.10).
    send_dm(
        &ctx.client,
        admin_keys,
        admin_keys,
        &ctx.mostro_pubkey,
        msg,
        None,
        false,
    )
    .await?;
    Ok(())
}
