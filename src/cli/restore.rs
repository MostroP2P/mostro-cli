use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::util::send_dm;

pub async fn execute_restore(
    identity_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    let restore_message = Message::new_restore(None);
    let message_json = restore_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the restore message to Mostro server
    send_dm(
        client,
        Some(identity_keys),
        identity_keys,
        &mostro_key,
        message_json,
        None,
        false,
    )
    .await?;

    println!("Restore message sent successfully. Recovering pending orders and disputes...");

    Ok(())
}
