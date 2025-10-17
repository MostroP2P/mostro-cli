use crate::cli::Context;
use crate::parser::dms::print_commands_results;
use crate::util::{send_dm, wait_for_dm};
use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

pub async fn execute_orders_info(order_ids: &[Uuid], ctx: &Context) -> Result<()> {
    if order_ids.is_empty() {
        return Err(anyhow::anyhow!("At least one order ID is required"));
    }

    println!("📋 Orders Information Request");
    println!("═══════════════════════════════════════");
    println!("📊 Number of Orders: {}", order_ids.len());
    println!("🆔 Order IDs:");
    for (i, order_id) in order_ids.iter().enumerate() {
        println!("  {}. {}", i + 1, order_id);
    }
    println!("🎯 Mostro PubKey: {}", ctx.mostro_pubkey);
    println!("💡 Requesting order information...");
    println!();

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create payload with the order IDs
    let payload = Payload::Ids(order_ids.to_vec());

    // Create message using the proper Message structure
    let message = Message::new_order(
        None,
        Some(request_id),
        Some(ctx.trade_index),
        Action::Orders,
        Some(payload),
    );

    // Serialize the message
    let message_json = message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the DM
    let sent_message = send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Wait for the DM response from mostro
    let recv_event = wait_for_dm(ctx, None, sent_message).await?;

    // Parse the incoming DM and handle the response
    let messages = crate::parser::dms::parse_dm_events(recv_event, &ctx.trade_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message_kind = message.get_inner_message_kind();

        // Check if this is the expected response
        if message_kind.request_id == Some(request_id) {
            print_commands_results(message_kind, ctx).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Received response with mismatched action. Expected: Orders, Got: {:?}",
                message_kind.action
            ));
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }

    Ok(())
}
