use crate::util::send_order_id_cmd;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_send_dm(
    trade_keys: &Keys,
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    send_order_id_cmd(
        client,
        trade_keys,
        trade_keys,
        receiver,
        message.to_string(),
        true,
        true,
    )
    .await?;

    Ok(())
}
