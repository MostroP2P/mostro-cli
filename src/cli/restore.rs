use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    parser::{dms::print_commands_results, parse_dm_events},
    util::{send_dm, wait_for_dm},
};

pub async fn execute_restore(
    identity_keys: &Keys,
    mostro_key: PublicKey,
    ctx: &Context,
) -> Result<()> {
    let restore_message = Message::new_restore(None);
    let message_json = restore_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the restore message to Mostro server
    let sent_message = send_dm(
        &ctx.client,
        Some(identity_keys),
        identity_keys,
        &mostro_key,
        message_json,
        None,
        false,
    );

    println!("🔄 Restore Session");
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
        Cell::new("👤 User"),
        Cell::new(identity_keys.public_key().to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Target"),
        Cell::new(mostro_key.to_string()),
    ]));
    println!("{table}");
    println!("💡 Sending restore request to Mostro...");
    println!("⏳ Recovering pending orders and disputes...\n");

    // Wait for incoming DM
    let recv_event = wait_for_dm(ctx, Some(identity_keys), sent_message).await?;

    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, identity_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message = message.get_inner_message_kind();
        if message.action == Action::RestoreSession {
            print_commands_results(message, ctx).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Received response with mismatched action. Expected: {:?}, Got: {:?}",
                Action::RestoreSession,
                message.action
            ))
        }
    } else {
        Err(anyhow::anyhow!("No response received from Mostro"))
    }
}
