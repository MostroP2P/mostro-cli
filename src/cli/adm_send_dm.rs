use crate::cli::Context;
use crate::util::send_admin_gift_wrap_dm;
use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use nostr_sdk::prelude::*;

pub async fn execute_adm_send_dm(receiver: PublicKey, ctx: &Context, message: &str) -> Result<()> {
    println!("👑 Admin Direct Message");
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
        Cell::new("🔑 Admin Keys"),
        Cell::new(ctx.context_keys.public_key().to_hex()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Recipient"),
        Cell::new(receiver.to_string()),
    ]));
    table.add_row(Row::from(vec![Cell::new("💬 Message"), Cell::new(message)]));
    println!("{table}");
    println!("💡 Sending admin gift wrap message...\n");

    send_admin_gift_wrap_dm(&ctx.client, &ctx.context_keys, &receiver, message).await?;

    println!(
        "✅ Admin gift wrap message sent successfully to {}",
        receiver
    );

    Ok(())
}
