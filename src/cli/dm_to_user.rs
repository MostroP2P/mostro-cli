use crate::db::Order;
use crate::parser::common::{
    print_info_line, print_key_value, print_section_header, print_success_message,
};
use anyhow::Result;
use mostro_core::chat::{derive_chat_keys, wrap_chat_message};
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn execute_dm_to_user(
    receiver: PublicKey,
    client: &Client,
    order_id: &Uuid,
    message: &str,
    pool: &SqlitePool,
) -> Result<()> {
    let message = message.trim();
    if message.is_empty() {
        anyhow::bail!("Cannot send empty chat message");
    }

    let order = Order::get_by_id(pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;
    let trade_keys = match order.trade_keys.as_ref() {
        Some(trade_keys) => Keys::parse(trade_keys)?,
        None => anyhow::bail!("No trade_keys found for this order"),
    };

    print_section_header("💬 Direct Message to User");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("🔑", "Trade Keys", &trade_keys.public_key().to_hex());
    print_key_value("🎯", "Recipient", &receiver.to_string());
    print_key_value("💬", "Message", message);
    print_info_line("💡", "Sending chat message...");
    println!();

    // protocol#52: kind 14 signed by K_sign, payload encrypted to K_conv.
    let (conv, sign) = derive_chat_keys(&trade_keys, &receiver)
        .map_err(|e| anyhow::anyhow!("Failed to derive chat keys: {e}"))?;
    let event = wrap_chat_message(&trade_keys, &conv, &sign, message)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wrap chat message: {e}"))?;
    client.send_event(&event).await?;
    print_success_message("Chat message sent!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::chat::unwrap_chat_message;

    #[tokio::test]
    async fn kind14_envelope_roundtrips_peer_to_peer() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let (conv, sign) = derive_chat_keys(&alice, &bob.public_key()).unwrap();
        let event = wrap_chat_message(&alice, &conv, &sign, "hello from alice")
            .await
            .unwrap();
        assert_eq!(event.kind, Kind::PrivateDirectMessage);
        assert_eq!(event.pubkey, sign.public_key());

        let allowed = [alice.public_key(), bob.public_key()];
        let chat = unwrap_chat_message(
            &conv,
            &sign.public_key(),
            &allowed,
            &event,
            Timestamp::now(),
        )
        .unwrap();
        assert_eq!(chat.content, "hello from alice");
        assert_eq!(chat.sender, alice.public_key());
    }
}
