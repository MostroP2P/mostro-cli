use crate::db::{Order, User};
use crate::util::send_message_sync;
use crate::{cli::Commands, db::connect};

use anyhow::Result;
use log::info;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
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
    // Get desised action based on command from CLI
    let requested_action = match command {
        Commands::FiatSent { order_id: _ } => Action::FiatSent,
        Commands::Release { order_id: _ } => Action::Release,
        Commands::Cancel { order_id: _ } => Action::Cancel,
        Commands::Dispute { order_id: _ } => Action::Dispute,
        Commands::AdmCancel { order_id: _ } => Action::AdminCancel,
        Commands::AdmSettle { order_id: _ } => Action::AdminSettle,
        Commands::AdmAddSolver { npubkey: _ } => Action::AdminAddSolver,
        _ => {
            println!("Not a valid command!");
            process::exit(0);
        }
    };

    println!(
        "Sending {} command for order {:?} to mostro pubId {}",
        requested_action,
        order_id,
        mostro_key.clone()
    );
    let mut payload = None;
    if let Some(t) = text {
        payload = Some(Payload::TextMessage(t.to_string()));
    }
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create message
    let message = Message::new_order(order_id, Some(request_id), None, requested_action, payload);
    info!("Sending message: {:#?}", message);

    let pool = connect().await?;
    let order = Order::get_by_id(&pool, &order_id.unwrap().to_string()).await;
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
                    false,
                    false,
                )
                .await?;
                let new_order = dm
                    .iter()
                    .find_map(|el| {
                        let message = el.0.get_inner_message_kind();
                        if message.request_id == Some(request_id) {
                            match message.action {
                                Action::NewOrder => {
                                    if let Some(Payload::Order(order)) = message.payload.as_ref() {
                                        return Some(order);
                                    }
                                }
                                _ => {
                                    return None;
                                }
                            }
                        }
                        None
                    })
                    .or_else(|| {
                        println!("Error: No matching order found in response");
                        None
                    });

                if let Some(order) = new_order {
                    println!("Order id {} created", order.id.unwrap());
                    // Create order in db
                    let pool = connect().await?;
                    let (trade_keys, trade_index) = User::get_next_trade_keys(&pool).await?;
                    let db_order =
                        Order::new(&pool, order.clone(), &trade_keys, Some(request_id as i64))
                            .await
                            .map_err(|e| anyhow::anyhow!("Failed to create DB order: {:?}", e))?;
                    let _ = db_order.id.clone().ok_or(anyhow::anyhow!(
                        "Failed getting new order from Mostro. Missing order id"
                    ))?;
                    // Update last trade index
                    match User::get(&pool).await {
                        Ok(mut user) => {
                            user.set_last_trade_index(trade_index + 1);
                            if let Err(e) = user.save(&pool).await {
                                println!("Failed to update user: {}", e);
                            }
                        }
                        Err(e) => println!("Failed to get user: {}", e),
                    }
                }
            } else {
                println!("Error: Missing trade keys for order {}", order_id.unwrap());
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    Ok(())
}
