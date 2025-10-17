use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use mostro_core::prelude::*;
use uuid::Uuid;

use crate::{cli::Context, util::admin_send_dm};

pub async fn execute_admin_add_solver(npubkey: &str, ctx: &Context) -> Result<()> {
    println!("👑 Admin Add Solver");
    println!("═══════════════════════════════════════");
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);
    table.add_row(Row::from(vec![
        Cell::new("🔑 Solver PubKey"),
        Cell::new(npubkey),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Mostro PubKey"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));
    println!("{table}");
    println!("💡 Adding new solver to Mostro...\n");
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

    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Solver added successfully!");

    Ok(())
}

pub async fn execute_admin_cancel_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Cancel Dispute");
    println!("═══════════════════════════════════════");
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);
    table.add_row(Row::from(vec![
        Cell::new("🆔 Dispute ID"),
        Cell::new(dispute_id.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Mostro PubKey"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));
    println!("{table}");
    println!("💡 Canceling dispute...\n");
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminCancel, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    println!("🔑 Admin Keys: {}", ctx.context_keys.public_key);

    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Dispute canceled successfully!");

    Ok(())
}

pub async fn execute_admin_settle_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Settle Dispute");
    println!("═══════════════════════════════════════");
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);
    table.add_row(Row::from(vec![
        Cell::new("🆔 Dispute ID"),
        Cell::new(dispute_id.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Mostro PubKey"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));
    println!("{table}");
    println!("💡 Settling dispute...\n");
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminSettle, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    println!("🔑 Admin Keys: {}", ctx.context_keys.public_key);
    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Dispute settled successfully!");
    Ok(())
}

pub async fn execute_take_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Take Dispute");
    println!("═══════════════════════════════════════");
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);
    table.add_row(Row::from(vec![
        Cell::new("🆔 Dispute ID"),
        Cell::new(dispute_id.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Mostro PubKey"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));
    println!("{table}");
    println!("💡 Taking dispute...\n");
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

    println!("🔑 Admin Keys: {}", ctx.context_keys.public_key);

    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Dispute taken successfully!");
    Ok(())
}
