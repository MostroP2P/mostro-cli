use anyhow::Result;
use cdk::nuts::SecretKey as CdkSecretKey;
use mostro_core::prelude::*;
use std::path::PathBuf;
use uuid::Uuid;

use crate::cashu::CashuWallet;
use crate::cli::Context;
use crate::db::Order;
use crate::util::events::fetch_events_list;
use crate::util::{Event, ListKind};

pub async fn execute_redeem_cashu_dispute(
    order_id: &Uuid,
    token: Option<&str>,
    ctx: &Context,
) -> Result<()> {
    let mint_url = ctx.mint_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Mint URL required: use --mint-url or set MINT_URL env var")
    })?;

    // Fetch all DMs from Mostro (no time filter) to find the CashuPmSignature message
    let dm_events =
        fetch_events_list(ListKind::DirectMessagesUser, None, None, None, ctx, None).await?;

    let pm_sigs: Vec<(String, String)> = dm_events
        .into_iter()
        .filter_map(|e| {
            if let Event::MessageTuple(tuple) = e {
                Some(*tuple)
            } else {
                None
            }
        })
        .find_map(|(msg, _, _)| {
            let inner = msg.get_inner_message_kind();
            if inner.action != Action::CashuPmSignature {
                return None;
            }
            if !inner.id.map(|id| id == *order_id).unwrap_or(false) {
                return None;
            }
            if let Some(Payload::CashuSignatures(sigs)) = &inner.payload {
                Some(
                    sigs.iter()
                        .map(|s| (s.secret.clone(), s.signature.clone()))
                        .collect(),
                )
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No CashuPmSignature message found in DMs for order {}",
                order_id
            )
        })?;

    // Get the escrow token: explicit --token arg takes priority, then DB
    let token_str = if let Some(t) = token {
        t.to_string()
    } else {
        let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("Order {} not found: {}", order_id, e))?;
        order.cashu_escrow_token.ok_or_else(|| {
            anyhow::anyhow!(
                "No escrow token in DB for order {}. Provide it via --token.",
                order_id
            )
        })?
    };

    // Inject Mostro's P_M signatures into each matching proof witness
    let token_with_pm = CashuWallet::inject_pm_signatures(&token_str, &pm_sigs)?;

    let trade_sk = ctx.trade_keys.secret_key();
    let own_cdk_sk = CdkSecretKey::from_slice(trade_sk.as_secret_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to derive CDK key: {}", e))?;

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mcli/cashu-wallet.redb");

    let sk_bytes = trade_sk.to_secret_bytes();
    let mut seed = [0u8; 64];
    seed[..32].copy_from_slice(&sk_bytes);
    seed[32..].copy_from_slice(&sk_bytes);

    let wallet = CashuWallet::new(mint_url, seed, &db_path)?;

    // wallet.receive() adds own signature; P_M sig already in witnesses → 2-of-3 met
    let amount = wallet
        .redeem_with_keys(&token_with_pm, vec![own_cdk_sk])
        .await?;

    println!(
        "Redeemed {} sats from disputed Cashu escrow for order {}.",
        amount, order_id
    );

    Ok(())
}
