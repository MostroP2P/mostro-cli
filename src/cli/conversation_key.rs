use anyhow::Result;
use nip44::v2::ConversationKey;
use nostr_sdk::prelude::*;
use std::str::FromStr;

use crate::util::send_relays_requests;

pub async fn execute_conversation_key(
    my_key: &Keys,
    receiver: PublicKey,
    client: &Client,
    event_id: &str,
) -> Result<()> {
    let id = EventId::from_str(event_id)?;
    let filter = Filter::new().id(id).limit(1);
    let mostro_req = send_relays_requests(client, filter).await;
    let event = mostro_req.first().unwrap().first().unwrap();
    // Derive gift wrap conversation key
    let gw_ck = ConversationKey::derive(my_key.secret_key()?, &event.pubkey);
    let gw_key = gw_ck.as_bytes();
    let mut gw_ck_hex = vec![];
    for i in gw_key {
        gw_ck_hex.push(format!("{:02x}", i));
    }
    let gw_ck_hex = gw_ck_hex.join("");
    // Derive seal conversation key
    let seal_ck = ConversationKey::derive(my_key.secret_key()?, &receiver);
    let seal_key = seal_ck.as_bytes();
    let mut seal_ck_hex = vec![];
    for i in seal_key {
        seal_ck_hex.push(format!("{:02x}", i));
    }
    let seal_ck_hex = seal_ck_hex.join("");
    println!("Gift wrap Conversation key: {:?}", gw_ck_hex);
    println!("Seal Conversation key: {:?}", seal_ck_hex);

    Ok(())
}
