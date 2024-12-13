use crate::util::send_message_sync;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_send_dm(
    trade_keys: &Keys,
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    send_message_sync(
        client,
        None,
        trade_keys,
        receiver,
        message.to_string(),
        true,
        true,
    )
    .await?;

    Ok(())
}
