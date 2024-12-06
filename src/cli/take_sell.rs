use anyhow::Result;
use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::secp256k1::Message as BitcoinMessage;
use hashes::Hash;
use lnurl::lightning_address::LightningAddress;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use std::str::FromStr;
use uuid::Uuid;

use crate::lightning::is_valid_invoice;
use crate::util::send_order_id_cmd;

#[allow(clippy::too_many_arguments)]
pub async fn execute_take_sell(
    order_id: &Uuid,
    invoice: &Option<String>,
    amount: Option<u32>,
    identity_keys: &Keys,
    trade_keys: &Keys,
    trade_index: u32,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take sell order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let mut content = None;
    if let Some(invoice) = invoice {
        // Check invoice string
        let ln_addr = LightningAddress::from_str(invoice);
        if ln_addr.is_ok() {
            content = Some(Content::PaymentRequest(None, invoice.to_string(), None));
        } else {
            match is_valid_invoice(invoice) {
                Ok(i) => content = Some(Content::PaymentRequest(None, i.to_string(), None)),
                Err(e) => println!("{}", e),
            }
        }
    }

    // Add amount in case it's specified
    if amount.is_some() {
        content = match content {
            Some(Content::PaymentRequest(a, b, _)) => {
                Some(Content::PaymentRequest(a, b, Some(amount.unwrap() as i64)))
            }
            None => Some(Content::Amount(amount.unwrap().into())),
            _ => None,
        };
    }
    // content should be sha256 hashed
    let json: Value = json!(content.clone().unwrap());
    let content_str: String = json.to_string();
    let hash: Sha256Hash = Sha256Hash::hash(content_str.as_bytes());
    let hash = hash.to_byte_array();
    let message: BitcoinMessage = BitcoinMessage::from_digest(hash);
    // content should be signed with the trade keys
    let sig = identity_keys.sign_schnorr(&message);
    // Create takesell message
    let take_sell_message = Message::new_order(
        None,
        None,
        Some(trade_index),
        Action::TakeSell,
        content,
        Some(sig),
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(
        client,
        identity_keys,
        trade_keys,
        mostro_key,
        take_sell_message,
        true,
        false,
    )
    .await?;
    Ok(())
}
