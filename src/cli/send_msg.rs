use crate::cli::{Commands, Context};
use crate::db::{Order, User};
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::parser::{dms::print_commands_results, parse_dm_events};
use crate::util::{
    create_filter, print_dm_events, send_dm, wait_for_dm, ListKind, FETCH_EVENTS_TIMEOUT,
};

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
        Commands::FiatSent { .. } => Action::FiatSent,
        Commands::Release { .. } => Action::Release,
        Commands::Cancel { .. } => Action::Cancel,
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

    // Determine payload
    let payload = match requested_action {
        Action::FiatSent | Action::Release => create_next_trade_payload(ctx, &order_id).await?,
        _ => text.map(|t| Payload::TextMessage(t.to_string())),
    };
    // Update last trade index if next trade payload
    if let Some(Payload::NextTrade(_, trade_index)) = &payload {
        // Update last trade index
        match User::get(&ctx.pool).await {
            Ok(mut user) => {
                user.set_last_trade_index(*trade_index as i64);
                if let Err(e) = user.save(&ctx.pool).await {
                    println!("Failed to update user: {}", e);
                }
            }
            Err(e) => println!("Failed to get user: {}", e),
        }
    }

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Clone values before they're moved into the message
    let requested_action_clone = requested_action.clone();
    let payload_clone = payload.clone();

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
                Some(&ctx.identity_keys),
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

            // For release actions, check if we need to wait for additional messages (new order creation)
            if requested_action_clone == Action::Release {
                // Check if this was a range order that might generate a new order
                if let Some(Payload::NextTrade(_, index)) = &payload_clone {
                    // Get the correct keys for decoding the child order message
                    let next_trade_key = User::get_trade_keys(&ctx.pool, *index as i64).await?;
                    // Fake timestamp for giftwraps
                    let subscription = create_filter(
                        ListKind::DirectMessagesUser,
                        next_trade_key.public_key,
                        None,
                    )?;

                    // Wait for potential new order message from Mostro
                    let events = ctx
                        .client
                        .fetch_events(subscription, FETCH_EVENTS_TIMEOUT)
                        .await?;
                    let messages = parse_dm_events(events, &next_trade_key, Some(&2)).await;
                    if !messages.is_empty() {
                        for (message, _, _) in messages {
                            let message_kind = message.get_inner_message_kind();
                            if message_kind.action == Action::NewOrder {
                                print_commands_results(message_kind, ctx).await?;
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn create_next_trade_payload(
    ctx: &Context,
    order_id: &Option<Uuid>,
) -> Result<Option<Payload>> {
    if let Some(order_id) = order_id {
        let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;

        if let (Some(_), Some(min_amount), Some(max_amount)) =
            (order.is_mine, order.min_amount, order.max_amount)
        {
            if max_amount - order.fiat_amount >= min_amount {
                let (trade_keys, trade_index) = User::get_next_trade_keys(&ctx.pool).await?;
                return Ok(Some(Payload::NextTrade(
                    trade_keys.public_key().to_string(),
                    trade_index.try_into()?,
                )));
            }
        }
    }
    Ok(None)
}
