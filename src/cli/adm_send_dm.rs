use crate::cli::Context;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::util::messaging::get_admin_keys;
use crate::util::send_plain_text_dm;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_adm_send_dm(receiver: PublicKey, ctx: &Context, message: &str) -> Result<()> {
    // Get admin keys
    let admin_keys = get_admin_keys(ctx)?;

    println!("👑 Admin Direct Message");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🔑 ",
        "Admin Keys",
        &admin_keys.public_key().to_hex(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Recipient",
        &receiver.to_string(),
    ));
    table.add_row(create_emoji_field_row("💬 ", "Message", message));
    println!("{table}");
    println!("💡 Sending admin gift wrap message...\n");

    send_plain_text_dm(&ctx.client, admin_keys, admin_keys, &receiver, message).await?;

    println!(
        "✅ Admin gift wrap message sent successfully to {}",
        receiver
    );

    Ok(())
}
