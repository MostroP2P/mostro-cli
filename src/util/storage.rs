use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::cli::send_msg::execute_send_msg;
use crate::cli::{Commands, Context};
use crate::db::{Order, User};

pub async fn save_order(
    order: SmallOrder,
    trade_keys: &Keys,
    request_id: u64,
    trade_index: i64,
    pool: &SqlitePool,
) -> Result<()> {
    let req_id_i64 = i64::try_from(request_id)
        .map_err(|_| anyhow::anyhow!("request_id too large for i64: {}", request_id))?;
    let order = Order::new(pool, order, trade_keys, Some(req_id_i64)).await?;

    if let Some(order_id) = order.id {
        println!("Order {} created", order_id);
    } else {
        println!("Warning: The newly created order has no ID.");
    }

    match User::get(pool).await {
        Ok(mut user) => {
            user.set_last_trade_index(trade_index);
            if let Err(e) = user.save(pool).await {
                println!("Failed to update user: {}", e);
            }
        }
        Err(e) => println!("Failed to get user: {}", e),
    }

    Ok(())
}

pub async fn run_simple_order_msg(
    command: Commands,
    order_id: Option<Uuid>,
    ctx: &Context,
) -> Result<()> {
    execute_send_msg(command, order_id, ctx, None).await
}

pub async fn admin_send_dm(ctx: &Context, msg: String) -> anyhow::Result<()> {
    super::messaging::send_dm(
        &ctx.client,
        Some(&ctx.context_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        msg,
        None,
        false,
    )
    .await?;
    Ok(())
}
