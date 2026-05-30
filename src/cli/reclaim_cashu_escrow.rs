use anyhow::Result;
use cdk::nuts::SecretKey as CdkSecretKey;
use nostr_sdk::prelude::*;
use std::path::PathBuf;
use uuid::Uuid;

use crate::cashu::CashuWallet;
use crate::cli::Context;
use crate::util::messaging::{derive_shared_key_bytes, fetch_gift_wraps_for_shared_key};

pub async fn execute_reclaim_cashu_escrow(
    order_id: &Uuid,
    buyer_pubkey: &str,
    ctx: &Context,
) -> Result<()> {
    let mint_url = ctx.mint_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Mint URL required: use --mint-url or set MINT_URL env var")
    })?;

    let buyer_nostr_pk = nostr_sdk::PublicKey::parse(buyer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid buyer pubkey: {}", e))?;

    let shared_key_bytes = derive_shared_key_bytes(&ctx.trade_keys, &buyer_nostr_pk)
        .map_err(|e| anyhow::anyhow!("Failed to derive shared key with buyer: {}", e))?;

    let shared_keys = Keys::new(
        SecretKey::from_slice(&shared_key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid shared key bytes: {}", e))?,
    );

    let messages = fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await?;

    // Find a cashu token sent by the buyer (P_B-signed, for cooperative cancel)
    let buyer_signed_token = messages
        .into_iter()
        .filter(|(_, _, sender)| *sender == buyer_nostr_pk)
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
                "No buyer-signed cancel token found in P2P channel for order {}",
                order_id
            )
        })?;

    let trade_sk = ctx.trade_keys.secret_key();
    let seller_cdk_sk = CdkSecretKey::from_slice(trade_sk.as_secret_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to derive seller CDK key: {}", e))?;

    // Add P_S signature on top of buyer's P_B signature (2-of-3 threshold met)
    let fully_signed_token = CashuWallet::sign_token(&buyer_signed_token, seller_cdk_sk.clone())?;

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mcli/cashu-wallet.redb");

    let sk_bytes = trade_sk.to_secret_bytes();
    let mut seed = [0u8; 64];
    seed[..32].copy_from_slice(&sk_bytes);
    seed[32..].copy_from_slice(&sk_bytes);

    let wallet = CashuWallet::new(mint_url, seed, &db_path)?;

    let amount = wallet
        .redeem_with_keys(&fully_signed_token, vec![seller_cdk_sk])
        .await?;

    println!(
        "Reclaimed {} sats from Cashu escrow for order {}.",
        amount, order_id
    );

    Ok(())
}
