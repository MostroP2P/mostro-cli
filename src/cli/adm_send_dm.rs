use crate::util::send_admin_gift_wrap_dm;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_adm_send_dm(
    receiver: PublicKey,
    client: &Client,
    message: &str,
) -> Result<()> {
    let admin_keys = match std::env::var("NSEC_PRIVKEY") {
        Ok(key) => Keys::parse(&key)?,
        Err(e) => {
            anyhow::bail!("NSEC_PRIVKEY not set: {e}");
        }
    };

    println!(
        "SENDING DM with admin keys: {}",
        admin_keys.public_key().to_hex()
    );

    send_admin_gift_wrap_dm(client, &admin_keys, &receiver, message).await?;

    println!("Admin gift wrap message sent to {}", receiver);

    Ok(())
}
