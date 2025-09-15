use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

use crate::{cli::Context, util::send_dm};

pub async fn execute_admin_add_solver(npubkey: &str, ctx: &Context) -> Result<()> {
    println!(
        "Request of add solver with pubkey {} from mostro pubId {}",
        npubkey, &ctx.mostro_pubkey
    );
    // Create takebuy message
    let take_dispute_message = Message::new_dispute(
        Some(Uuid::new_v4()),
        None,
        None,
        Action::AdminAddSolver,
        Some(Payload::TextMessage(npubkey.to_string())),
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_admin_cancel_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!(
        "Request of cancel dispute {} from mostro pubId {}",
        dispute_id,
        ctx.mostro_pubkey.clone()
    );
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminCancel, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    println!(
        "identity_keys: {:?}",
        ctx.identity_keys.public_key.to_string()
    );

    send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_admin_settle_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        ctx.mostro_pubkey.clone()
    );
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminSettle, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    println!(
        "identity_keys: {:?}",
        ctx.identity_keys.public_key.to_string()
    );

    send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    )
    .await?;

    Ok(())
}

pub async fn execute_take_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        ctx.mostro_pubkey.clone()
    );
    // Create takebuy message
    let take_dispute_message = Message::new_dispute(
        Some(*dispute_id),
        None,
        None,
        Action::AdminTakeDispute,
        None,
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    println!(
        "identity_keys: {:?}",
        ctx.identity_keys.public_key.to_string()
    );

    send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    )
    .await?;

    Ok(())
}
