use anyhow::Result;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::send_message_sync;

pub async fn execute_admin_add_solver(
    npubkey: &str,
    identity_keys: &Keys,
    trade_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of add solver with pubkey {} from mostro pubId {}",
        npubkey,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_dispute_message = Message::new_dispute(
        Some(Uuid::new_v4()),
        None,
        None,
        Action::AdminAddSolver,
        Some(Payload::TextMessage(npubkey.to_string())),
    );

    send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_dispute_message,
        true,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_admin_cancel_dispute(
    dispute_id: &Uuid,
    identity_keys: &Keys,
    trade_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of cancel dispute {} from mostro pubId {}",
        dispute_id,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminCancel, None);

    println!("identity_keys: {:?}", identity_keys.public_key.to_string());

    send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_dispute_message,
        true,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_admin_settle_dispute(
    dispute_id: &Uuid,
    identity_keys: &Keys,
    trade_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminSettle, None);

    println!("identity_keys: {:?}", identity_keys.public_key.to_string());

    send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_dispute_message,
        true,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_take_dispute(
    dispute_id: &Uuid,
    identity_keys: &Keys,
    trade_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_dispute_message = Message::new_dispute(
        Some(*dispute_id),
        None,
        None,
        Action::AdminTakeDispute,
        None,
    );

    println!("identity_keys: {:?}", identity_keys.public_key.to_string());

    send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_dispute_message,
        true,
        false,
    )
    .await?;

    Ok(())
}
