use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use crate::db::{connect, User};
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
    let mut payload = None;
    if let Some(invoice) = invoice {
        // Check invoice string
        let ln_addr = LightningAddress::from_str(invoice);
        if ln_addr.is_ok() {
            payload = Some(Payload::PaymentRequest(None, invoice.to_string(), None));
        } else {
            match is_valid_invoice(invoice) {
                Ok(i) => payload = Some(Payload::PaymentRequest(None, i.to_string(), None)),
                Err(e) => println!("{}", e),
            }
        }
    }

    // Add amount in case it's specified
    if amount.is_some() {
        payload = match payload {
            Some(Payload::PaymentRequest(a, b, _)) => {
                Some(Payload::PaymentRequest(a, b, Some(amount.unwrap() as i64)))
            }
            None => Some(Payload::Amount(amount.unwrap().into())),
            _ => None,
        };
    }
    // Create takesell message
    let take_sell_message = Message::new_order(
        None,
        None,
        Some(trade_index.into()),
        Action::TakeSell,
        payload,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_sell_message,
        true,
        false,
    )
    .await?;

    let pool = connect().await?;
    // Update last trade index
    let mut user = User::get(&pool).await.unwrap();
    user.set_last_trade_index(trade_index as i64);
    user.save(&pool).await.unwrap();

    Ok(())
}
