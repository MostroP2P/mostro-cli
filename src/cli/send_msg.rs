use crate::db::{Order, User};
use crate::util::send_message_sync;
use crate::{cli::Commands, db::connect};

use anyhow::Result;
use log::info;
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

    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create and send the message
    let message = Message::new_order(order_id, Some(request_id), None, requested_action, payload);
    info!("Sending message: {:#?}", message);

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
    if let Some(order) = dm.iter().find_map(|el| {
        let message = el.0.get_inner_message_kind();
        if message.request_id == Some(request_id) {
            if let Some(Payload::Order(order)) = message.payload.as_ref() {
                return Some(order.clone());
            }
        }
        None
    }) {
        println!("Order id {} created", order.id.unwrap());
        Order::new(pool, order.clone(), trade_keys, Some(request_id as i64)).await?;
    } else {
        println!("Error: No matching order found in response");
    }

    Ok(())
}
