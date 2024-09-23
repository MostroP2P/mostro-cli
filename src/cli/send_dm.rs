use crate::util::send_order_id_cmd;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_send_dm(
    my_key: &Keys,
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    send_order_id_cmd(client, my_key, receiver, message.to_string(), true, true).await?;

    Ok(())
}
