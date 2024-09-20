use crate::util::send_dm;
use log::info;
use nostr_sdk::prelude::*;

pub async fn execute_send_dm(
    my_key: &Keys,
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    send_dm(client, my_key, &receiver, message.to_string()).await?;
    let mut notifications = client.notifications();
    if let Ok(notification) = notifications.recv().await {
        info!("Notification received: {:#?}", notification);
    }

    Ok(())
}
