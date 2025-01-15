use crate::db::{Order, User};
use crate::util::send_message_sync;
use crate::{cli::Commands, db::connect};

use anyhow::Result;
use mostro_core::message::{Action, Message, Payload};
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
    // println!("Sending message: {:#?}", message);

    if let Some(order_id) = order_id {
        handle_order_response(
            &pool,
            client,
            identity_keys,
            mostro_key,
            message,
            order_id,
            request_id,
        )
        .await?;
    } else {
        println!("Error: Missing order ID");
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
            if max_amount - order.amount >= min_amount {
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

async fn handle_order_response(
    pool: &SqlitePool,
    client: &Client,
    identity_keys: Option<&Keys>,
    mostro_key: PublicKey,
    message: Message,
    order_id: Uuid,
    request_id: u64,
) -> Result<()> {
    let order = Order::get_by_id(pool, &order_id.to_string()).await;

    match order {
        Ok(order) => {
            if let Some(trade_keys_str) = order.trade_keys {
                let trade_keys = Keys::parse(&trade_keys_str)?;
                let dm = send_message_sync(
                    client,
                    identity_keys,
                    &trade_keys,
                    mostro_key,
                    message,
                    true,
                    false,
                )
                .await?;
                process_order_response(dm, pool, &trade_keys, request_id).await?;
            } else {
                println!("Error: Missing trade keys for order {}", order_id);
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    Ok(())
}

async fn process_order_response(
    dm: Vec<(Message, u64)>,
    pool: &SqlitePool,
    trade_keys: &Keys,
    request_id: u64,
) -> Result<()> {
    for (message, _) in dm {
        let kind = message.get_inner_message_kind();
        if let Some(req_id) = kind.request_id {
            if req_id != request_id {
                continue;
            }

            match kind.action {
                Action::NewOrder => {
                    if let Some(Payload::Order(order)) = kind.payload.as_ref() {
                        Order::new(pool, order.clone(), trade_keys, Some(request_id as i64))
                            .await
                            .map_err(|e| anyhow::anyhow!("Failed to create new order: {}", e))?;
                        return Ok(());
                    }
                }
                Action::Canceled => {
                    if let Some(id) = kind.id {
                        // Verify order exists before deletion
                        if Order::get_by_id(pool, &id.to_string()).await.is_ok() {
                            Order::delete_by_id(pool, &id.to_string())
                                .await
                                .map_err(|e| anyhow::anyhow!("Failed to delete order: {}", e))?;
                            return Ok(());
                        } else {
                            return Err(anyhow::anyhow!("Order not found: {}", id));
                        }
                    }
                }
                _ => (),
            }
        }
    }

    Ok(())
}
