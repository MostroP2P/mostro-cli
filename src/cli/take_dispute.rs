use crate::util::messaging::get_admin_keys;
use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

use crate::{
    cli::Context,
    parser::common::{create_emoji_field_row, create_field_value_header, create_standard_table},
    parser::{dms::print_commands_results, parse_dm_events},
    util::{admin_send_dm, send_dm, wait_for_dm},
};

pub async fn execute_admin_add_solver(npubkey: &str, ctx: &Context) -> Result<()> {
    println!("👑 Admin Add Solver");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row("🔑 ", "Solver PubKey", npubkey));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Adding new solver to Mostro...\n");

    let _admin_keys = get_admin_keys(ctx)?;

    // Build admin dispute message
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
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Canceling dispute...\n");

    let _admin_keys = get_admin_keys(ctx)?;

    // Build admin dispute message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminCancel, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Dispute canceled successfully!");

    Ok(())
}

pub async fn execute_admin_settle_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Settle Dispute");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Settling dispute...\n");

    let _admin_keys = get_admin_keys(ctx)?;

    // Build admin dispute message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminSettle, None)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;
    admin_send_dm(ctx, take_dispute_message).await?;

    println!("✅ Dispute settled successfully!");
    Ok(())
}

pub async fn execute_take_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Take Dispute");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Taking dispute...\n");

    let admin_keys = get_admin_keys(ctx)?;

    // Build admin dispute message
    let take_dispute_message = Message::new_dispute(
        Some(*dispute_id),
        None,
        None,
        Action::AdminTakeDispute,
        None,
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the dispute message and wait for response. Admin identity
    // binds via the rumor/seal/inner-signature produced from `admin_keys`.
    let sent_message = send_dm(
        &ctx.client,
        admin_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    );

    // Wait for incoming DM response
    let recv_event = wait_for_dm(ctx, Some(admin_keys), sent_message).await?;

    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, admin_keys, None).await;
    if let Some((message, _, sender_pubkey)) = messages.first() {
        let message_kind = message.get_inner_message_kind();
        if *sender_pubkey != ctx.mostro_pubkey {
            return Err(anyhow::anyhow!("Received response from wrong sender"));
        }
        if message_kind.action == Action::AdminTookDispute {
            print_commands_results(message_kind, ctx).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Received response with mismatched action. Expected: {:?}, Got: {:?}",
                Action::AdminTookDispute,
                message_kind.action
            ));
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }

    Ok(())
}
