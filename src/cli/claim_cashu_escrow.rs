use anyhow::Result;
use cdk::nuts::SecretKey as CdkSecretKey;
use std::path::PathBuf;
use uuid::Uuid;

use crate::cashu::CashuWallet;
use crate::cli::Context;
use crate::util::messaging::{
    derive_shared_key_bytes, fetch_gift_wraps_for_shared_key,
};
use nostr_sdk::prelude::*;

pub async fn execute_claim_cashu_escrow(
    order_id: &Uuid,
    seller_pubkey: &str,
    ctx: &Context,
) -> Result<()> {
    let mint_url = ctx.mint_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Mint URL required: use --mint-url or set MINT_URL env var")
    })?;

    let seller_nostr_pk = nostr_sdk::PublicKey::parse(seller_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid seller pubkey: {}", e))?;

    let shared_key_bytes =
        derive_shared_key_bytes(&ctx.trade_keys, &seller_nostr_pk)
            .map_err(|e| anyhow::anyhow!("Failed to derive shared key with seller: {}", e))?;

    let shared_keys = Keys::new(
        SecretKey::from_slice(&shared_key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid shared key bytes: {}", e))?,
    );

    let messages = fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await?;

    let signed_token = messages
        .into_iter()
        .filter(|(_, _, sender)| *sender == seller_nostr_pk)
        .find_map(|(content, _, _)| {
            let trimmed = content.trim().to_string();
            if trimmed.starts_with("cashu") {
                Some(trimmed)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No Cashu release token found in P2P channel for order {}",
                order_id
            )
        })?;

    let trade_sk = ctx.trade_keys.secret_key();
    let buyer_cdk_sk = CdkSecretKey::from_slice(trade_sk.as_secret_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to derive buyer CDK key: {}", e))?;

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mcli/cashu-wallet.redb");

    let sk_bytes = trade_sk.to_secret_bytes();
    let mut seed = [0u8; 64];
    seed[..32].copy_from_slice(&sk_bytes);
    seed[32..].copy_from_slice(&sk_bytes);

    let wallet = CashuWallet::new(mint_url, seed, &db_path)?;

    let amount = wallet
        .redeem_with_keys(&signed_token, vec![buyer_cdk_sk])
        .await?;

    println!("Claimed {} sats from Cashu escrow for order {}.", amount, order_id);

    Ok(())
}
