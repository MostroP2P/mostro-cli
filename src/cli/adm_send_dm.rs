use crate::cli::Context;
use crate::util::send_admin_gift_wrap_dm;
use anyhow::Result;
use nostr_sdk::prelude::*;

pub async fn execute_adm_send_dm(receiver: PublicKey, ctx: &Context, message: &str) -> Result<()> {
    println!(
        "SENDING DM with admin keys: {}",
        ctx.context_keys.public_key().to_hex()
    );

    send_admin_gift_wrap_dm(&ctx.client, &ctx.context_keys, &receiver, message).await?;

    println!("Admin gift wrap message sent to {}", receiver);

    Ok(())
}
