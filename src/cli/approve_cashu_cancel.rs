use anyhow::Result;
use cdk::nuts::SecretKey as CdkSecretKey;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::cashu::CashuWallet;
use crate::cli::Context;
use crate::util::messaging::{
    derive_shared_key_bytes, derive_shared_keys, fetch_gift_wraps_for_shared_key,
    send_admin_chat_message_via_shared_key,
};

pub async fn execute_approve_cashu_cancel(
    order_id: &Uuid,
    seller_pubkey: &str,
    ctx: &Context,
) -> Result<()> {
    let seller_nostr_pk = nostr_sdk::PublicKey::parse(seller_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid seller pubkey: {}", e))?;

    let shared_key_bytes = derive_shared_key_bytes(&ctx.trade_keys, &seller_nostr_pk)
        .map_err(|e| anyhow::anyhow!("Failed to derive shared key with seller: {}", e))?;

    let shared_keys = Keys::new(
        SecretKey::from_slice(&shared_key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid shared key bytes: {}", e))?,
    );

    let messages = fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await?;

    let token_str = messages
        .into_iter()
        .filter(|(_, _, sender)| *sender == seller_nostr_pk)
        .find_map(|(content, _, _)| {
            content
                .trim()
                .strip_prefix("CANCEL:")
                .map(|t| t.to_string())
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No cancel request token found in P2P channel for order {}",
                order_id
            )
        })?;

    let trade_sk = ctx.trade_keys.secret_key();
    let buyer_cdk_sk = CdkSecretKey::from_slice(trade_sk.as_secret_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to derive buyer CDK key: {}", e))?;

    let signed_token = CashuWallet::sign_token(&token_str, buyer_cdk_sk)?;

    let seller_shared_keys = derive_shared_keys(Some(&ctx.trade_keys), Some(&seller_nostr_pk))
        .ok_or_else(|| anyhow::anyhow!("Failed to derive shared key with seller"))?;

    send_admin_chat_message_via_shared_key(
        &ctx.client,
        &ctx.trade_keys,
        &seller_shared_keys,
        &signed_token,
    )
    .await?;

    println!(
        "Cancel approved: signed token sent back to seller {}.",
        &seller_pubkey[..16]
    );

    Ok(())
}
