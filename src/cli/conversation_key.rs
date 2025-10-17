use anyhow::Result;
use nip44::v2::ConversationKey;
use nostr_sdk::prelude::*;

pub async fn execute_conversation_key(trade_keys: &Keys, receiver: PublicKey) -> Result<()> {
    println!("🔐 Conversation Key Generator");
    println!("═══════════════════════════════════════");
    println!("🔑 Trade Keys: {}", trade_keys.public_key().to_hex());
    println!("🎯 Receiver: {}", receiver);
    println!("💡 Deriving conversation key...");
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
    println!("✅ Conversation key generated successfully!");

    Ok(())
}
