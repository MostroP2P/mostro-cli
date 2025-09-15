use crate::cli::{Commands, Context};
use crate::db::{Order, User};
use crate::util::{send_dm, wait_for_dm};

use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::process;
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
            eprintln!("Not a valid command!");
            process::exit(0);
        }
    };

    println!(
        "Sending {} command for order {:?} to mostro pubId {}",
        requested_action,
        order_id.as_ref(),
        &ctx.mostro_pubkey
    );

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

    // Create and send the message
    let message = Message::new_order(order_id, Some(request_id), None, requested_action, payload);
    let idkey = ctx.identity_keys.to_owned();

    if let Some(order_id) = order_id {
        let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;

        if let Some(trade_keys_str) = order.trade_keys.clone() {
            let trade_keys = Keys::parse(&trade_keys_str)?;
            // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
            let subscription = Filter::new()
                .pubkey(trade_keys.public_key())
                .kind(nostr_sdk::Kind::GiftWrap)
                .limit(0);

            let opts =
                SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEvents(1));
            // Subscribe to gift wrap events
            ctx.client.subscribe(subscription, Some(opts)).await?;
            // Send DM
            let message_json = message
                .as_json()
                .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?;
            send_dm(
                &ctx.client,
                Some(&idkey),
                &trade_keys,
                &ctx.mostro_pubkey,
                message_json,
                None,
                false,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send DM: {e}"))?;

            // Wait for the DM to be sent from mostro
            wait_for_dm(
                &ctx.client,
                &trade_keys,
                request_id,
                None,
                Some(order),
                &ctx.pool,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to wait for DM: {e}"))?;
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
