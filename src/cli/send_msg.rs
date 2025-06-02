use crate::db::{Order, User};
use crate::util::wait_for_dm;
use crate::{cli::Commands, db::connect};

use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::process;
use uuid::Uuid;

pub async fn execute_send_msg(
    command: Commands,
    order_id: Option<Uuid>,
    identity_keys: Option<&Keys>,
    mostro_key: PublicKey,
    client: &Client,
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
        requested_action, order_id, mostro_key
    );

    let pool = connect().await?;

    // Determine payload
    let payload = match requested_action {
        Action::FiatSent | Action::Release => create_next_trade_payload(&pool, &order_id).await?,
        _ => text.map(|t| Payload::TextMessage(t.to_string())),
    };
    // Update last trade index if next trade payload
    if let Some(Payload::NextTrade(_, trade_index)) = &payload {
        // Update last trade index
        match User::get(&pool).await {
            Ok(mut user) => {
                user.set_last_trade_index(*trade_index as i64);
                if let Err(e) = user.save(&pool).await {
                    println!("Failed to update user: {}", e);
                }
            }
            Err(e) => println!("Failed to get user: {}", e),
        }
    }

    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create and send the message
    let message = Message::new_order(order_id, Some(request_id), None, requested_action, payload);
    let client_clone = client.clone();
    let idkey = identity_keys.unwrap().to_owned();

    if let Some(order_id) = order_id {
        let order = Order::get_by_id(&pool, &order_id.to_string()).await?;

        if let Some(trade_keys_str) = order.trade_keys.clone() {
            let trade_keys = Keys::parse(&trade_keys_str)?;
            // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
            let subscription = Filter::new()
                .pubkey(trade_keys.public_key())
                .kind(nostr_sdk::Kind::GiftWrap)
                .limit(0);

            let opts =
                SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEvents(1));

            client.subscribe(subscription, Some(opts)).await?;
            // Clone the keys and client for the async call
            let trade_keys_clone = trade_keys.clone();

            // Spawn a new task to send the DM
            // This is so we can wait for the gift wrap event in the main thread
            tokio::spawn(async move {
                let _ = crate::util::send_dm(
                    &client_clone,
                    Some(&idkey),
                    &trade_keys_clone,
                    &mostro_key,
                    message.as_json().unwrap(),
                    None,
                    false,
                )
                .await;
            });

            // Wait for the DM to be sent from mostro
            wait_for_dm(client, &trade_keys, request_id, 0, Some(order)).await?;
        }
    }

    Ok(())
}

async fn create_next_trade_payload(
    pool: &SqlitePool,
    order_id: &Option<Uuid>,
) -> Result<Option<Payload>> {
    if let Some(order_id) = order_id {
        let order = Order::get_by_id(pool, &order_id.to_string()).await?;

        if let (Some(_), Some(min_amount), Some(max_amount)) =
            (order.is_mine, order.min_amount, order.max_amount)
        {
            if max_amount - order.fiat_amount >= min_amount {
                let (trade_keys, trade_index) = User::get_next_trade_keys(pool).await?;
                return Ok(Some(Payload::NextTrade(
                    trade_keys.public_key().to_string(),
                    trade_index.try_into()?,
                )));
            }
        }
    }
    Ok(None)
}
