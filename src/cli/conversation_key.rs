use crate::parser::common::{
    print_info_line, print_key_value, print_section_header, print_success_message,
};
use anyhow::Result;
use nip44::v2::ConversationKey;
use nostr_sdk::prelude::*;

pub async fn execute_conversation_key(trade_keys: &Keys, receiver: PublicKey) -> Result<()> {
    print_section_header("🔐 Conversation Key Generator");
    print_key_value("🔑", "Trade Keys", &trade_keys.public_key().to_hex());
    print_key_value("🎯", "Receiver", &receiver.to_string());
    print_info_line("💡", "Deriving conversation key...");
    println!();

    // Derive conversation key
    let ck = ConversationKey::derive(trade_keys.secret_key(), &receiver)?;
    let key = ck.as_bytes();
    let mut ck_hex = vec![];
    for i in key {
        ck_hex.push(format!("{:02x}", i));
    }
    let ck_hex = ck_hex.join("");

    println!("🔐 Conversation Key:");
    println!("─────────────────────────────────────");
    println!("{}", ck_hex);
    println!("─────────────────────────────────────");
    print_success_message("Conversation key generated successfully!");

    Ok(())
}
