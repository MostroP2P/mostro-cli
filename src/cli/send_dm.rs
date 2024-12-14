use crate::util::send_message_sync;
use anyhow::Result;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;

pub async fn execute_send_dm(
    trade_keys: &Keys,
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    let message = Message::new_dm(
        None,
        None,
        Action::SendDm,
        Some(Payload::TextMessage(message.to_string())),
    );
    send_message_sync(client, None, trade_keys, receiver, message, true, true).await?;

    Ok(())
}
