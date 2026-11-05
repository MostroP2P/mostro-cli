use crate::cli::{Commands, Context};
use crate::db::Order;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::util::{print_dm_events, send_dm, wait_for_dm};

use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

pub async fn execute_send_msg(
    command: Commands,
    order_id: Option<Uuid>,
    ctx: &Context,
    text: Option<&str>,
) -> Result<()> {
    // Map CLI command to action
    let requested_action = match command {
        Commands::Dispute { .. } => Action::Dispute,
        Commands::AdmCancel { .. } => Action::AdminCancel,
        Commands::AdmSettle { .. } => Action::AdminSettle,
        Commands::AdmAddSolver { .. } => Action::AdminAddSolver,
        _ => {
            return Err(anyhow::anyhow!("Invalid command for send msg"));
        }
    };

    // Printout command information
    println!("📤 Send Message Command");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Action",
        &requested_action.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order ID",
        &order_id.map_or_else(|| "N/A".to_string(), |id| id.to_string()),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Target",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Sending command to Mostro...\n");

    // The payload is the text of the dispute, when there is one
    let payload = text.map(|t| Payload::TextMessage(t.to_string()));

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create and send the message
    let message = Message::new_order(order_id, Some(request_id), None, requested_action, payload);

    if let Some(order_id) = order_id {
        let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;

        if let Some(trade_keys_str) = order.trade_keys.clone() {
            let trade_keys = Keys::parse(&trade_keys_str)?;

            // Send DM
            let message_json = message
                .as_json()
                .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?;

            // Send DM
            let sent_message = send_dm(
                &ctx.client,
                &ctx.identity_keys,
                &trade_keys,
                &ctx.mostro_pubkey,
                message_json,
                None,
                false,
            );

            // Wait for incoming DM
            let recv_event = wait_for_dm(ctx, Some(&trade_keys), sent_message).await?;

            // Parse the incoming DM
            print_dm_events(recv_event, request_id, ctx, Some(&trade_keys)).await?;
        }
    }
    Ok(())
}
