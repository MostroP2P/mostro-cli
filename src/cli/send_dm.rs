use crate::cli::Context;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::{db::Order, util::send_dm};
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

pub async fn execute_send_dm(
    receiver: PublicKey,
    ctx: &Context,
    order_id: &Uuid,
    message: &str,
) -> Result<()> {
    println!("💬 Send Direct Message");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order ID",
        &order_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Recipient",
        &receiver.to_string(),
    ));
    table.add_row(create_emoji_field_row("💬 ", "Message", message));
    println!("{table}");
    println!("💡 Sending direct message...\n");

    let message = Message::new_dm(
        None,
        None,
        Action::SendDm,
        Some(Payload::TextMessage(message.to_string())),
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    let trade_keys =
        if let Ok(order_to_vote) = Order::get_by_id(&ctx.pool, &order_id.to_string()).await {
            match order_to_vote.trade_keys.as_ref() {
                Some(trade_keys) => Keys::parse(trade_keys)?,
                None => {
                    anyhow::bail!("No trade_keys found for this order");
                }
            }
        } else {
            return Err(anyhow::anyhow!("order {} not found", order_id));
        };

    send_dm(
        &ctx.client,
        Some(&trade_keys),
        &trade_keys,
        &receiver,
        message,
        None,
        false,
    )
    .await?;

    println!("✅ Direct message sent successfully!");

    Ok(())
}
