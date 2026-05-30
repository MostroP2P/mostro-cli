use anyhow::Result;
use cdk::nuts::{PublicKey as CdkPublicKey, SecretKey as CdkSecretKey};
use mostro_core::prelude::*;
use std::path::PathBuf;
use uuid::Uuid;

use crate::cashu::CashuWallet;
use crate::cli::Context;
use crate::db::Order;
use crate::util::{print_dm_events, send_dm, wait_for_dm};

fn nostr_pk_to_cdk(pk: &nostr_sdk::PublicKey) -> Result<CdkPublicKey> {
    let mut compressed = [0u8; 33];
    compressed[0] = 0x02;
    compressed[1..].copy_from_slice(pk.as_bytes());
    CdkPublicKey::from_slice(&compressed)
        .map_err(|e| anyhow::anyhow!("Failed to convert pubkey: {}", e))
}

pub async fn execute_add_cashu_escrow(
    order_id: &Uuid,
    amount: u64,
    buyer_pubkey: &str,
    ctx: &Context,
) -> Result<()> {
    let mint_url = ctx.mint_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Mint URL required: use --mint-url or set MINT_URL env var")
    })?;

    let buyer_nostr_pk = nostr_sdk::PublicKey::parse(buyer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid buyer pubkey: {}", e))?;
    let buyer_cdk_pk = nostr_pk_to_cdk(&buyer_nostr_pk)?;
    let mostro_cdk_pk = nostr_pk_to_cdk(&ctx.mostro_pubkey)?;

    let trade_sk = ctx.trade_keys.secret_key();
    let seller_cdk_sk = CdkSecretKey::from_slice(trade_sk.as_secret_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to derive seller CDK secret key: {}", e))?;
    let seller_cdk_pk = seller_cdk_sk.public_key();

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mcli/cashu-wallet.redb");

    // Derive a deterministic wallet seed from the trade secret key
    let sk_bytes = trade_sk.to_secret_bytes();
    let mut seed = [0u8; 64];
    seed[..32].copy_from_slice(&sk_bytes);
    seed[32..].copy_from_slice(&sk_bytes);

    let wallet = CashuWallet::new(mint_url, seed, &db_path)?;

    println!("Locking {} sats as Cashu 2-of-3 P2PK escrow...", amount);

    let token = wallet
        .swap_to_p2pk_locked(amount, buyer_cdk_pk, seller_cdk_pk, mostro_cdk_pk)
        .await?;

    Order::save_cashu_escrow(&ctx.pool, &order_id.to_string(), mint_url, &token).await?;

    let lock_proof = CashuLockProof::new(
        token,
        mint_url.to_string(),
        buyer_nostr_pk.to_hex(),
        ctx.trade_keys.public_key().to_hex(),
        ctx.mostro_pubkey.to_hex(),
    );

    let request_id = Uuid::new_v4().as_u128() as u64;

    let message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(ctx.trade_index),
        Action::AddCashuEscrow,
        Some(Payload::CashuLockProof(lock_proof)),
    );

    let message_json = message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    let recv_event = wait_for_dm(ctx, None, sent_message).await?;
    print_dm_events(recv_event, request_id, ctx, None).await?;

    Ok(())
}
