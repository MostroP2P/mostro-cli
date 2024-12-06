use crate::nip33::{dispute_from_tags, order_from_tags};
use crate::nip59::{gift_wrap, unwrap_gift_wrap};

use anyhow::{Error, Result};
use base64::engine::general_purpose;
use base64::Engine;
use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::Message as BitcoinMessage;
use chrono::DateTime;
use dotenvy::var;
use log::{error, info};
use mostro_core::dispute::Dispute;
use mostro_core::message::{Content, Message};
use mostro_core::order::Kind as MostroKind;
use mostro_core::order::{SmallOrder, Status};
use mostro_core::NOSTR_REPLACEABLE_EVENT_KIND;
use nip44::v2::{decrypt_to_bytes, encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use std::time::Duration;
use std::{fs, path::Path};
use tokio::time::timeout;
use uuid::Uuid;

pub async fn send_dm(
    client: &Client,
    identity_keys: Option<&Keys>,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    content: String,
    to_user: bool,
) -> Result<()> {
    let pow: u8 = var("POW").unwrap_or('0'.to_string()).parse().unwrap();
    let event = if to_user {
        // Derive conversation key
        let ck = ConversationKey::derive(trade_keys.secret_key(), receiver_pubkey);
        // Encrypt content
        let encrypted_content = encrypt_to_bytes(&ck, content)?;
        // Encode with base64
        let b64decoded_content = general_purpose::STANDARD.encode(encrypted_content);
        // Compose builder
        EventBuilder::new(Kind::PrivateDirectMessage, b64decoded_content)
            .pow(pow)
            .tag(Tag::public_key(*receiver_pubkey))
            .sign_with_keys(trade_keys)?
    } else {
        let identity_keys = identity_keys
            .ok_or_else(|| Error::msg("identity_keys required when to_user is false"))?;
        gift_wrap(
            identity_keys,
            trade_keys,
            *receiver_pubkey,
            content,
            None,
            pow,
        )?
    };

    info!("Sending event: {event:#?}");
    client.send_event(event).await?;

    Ok(())
}

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = Keys::generate();

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    // Create new client
    let client = Client::new(my_keys);
    // Add relays
    for r in relays.into_iter() {
        client.add_relay(r).await?;
    }
    // Connect to relays and keep connection alive
    client.connect().await;

    Ok(client)
}

pub async fn send_order_id_cmd(
    client: &Client,
    identity_keys: Option<&Keys>,
    trade_keys: &Keys,
    receiver_pubkey: PublicKey,
    message: String,
    wait_for_dm_ans: bool,
    to_user: bool,
) -> Result<()> {
    // Send dm to receiver pubkey
    send_dm(
        client,
        identity_keys,
        trade_keys,
        &receiver_pubkey,
        message,
        to_user,
    )
    .await?;

    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if wait_for_dm_ans {
            let dm = get_direct_messages(client, trade_keys, 1, to_user).await;

            for el in dm.iter() {
                match Message::from_json(&el.0) {
                    Ok(m) => {
                        if let Some(Content::PaymentRequest(ord, inv, _)) =
                            &m.get_inner_message_kind().content.0
                        {
                            println!("NEW MESSAGE:");
                            println!(
                                "Mostro sent you this hold invoice for order id: {}",
                                ord.as_ref().unwrap().id.unwrap()
                            );
                            println!();
                            println!("Pay this invoice to continue -->  {}", inv);
                            println!();
                        }
                    }
                    Err(_) => {
                        println!("NEW MESSAGE:");
                        println!();
                        println!("You got this message -->  {}", el.0);
                        println!();
                    }
                }
            }
            break;
        } else if let RelayPoolNotification::Message {
            message:
                RelayMessage::Ok {
                    event_id: _,
                    status: _,
                    message: _,
                },
            ..
        } = notification
        {
            println!(
                "Message correctly sent to Mostro! Check messages with getdm or listorders command"
            );
            break;
        }
    }
    Ok(())
}

pub async fn requests_relay(
    client: Client,
    relay: (RelayUrl, Relay),
    filters: Filter,
) -> Vec<Event> {
    let relrequest = get_events_of_mostro(&relay.1, vec![filters.clone()], client);

    // Buffer vector
    let mut res: Vec<Event> = Vec::new();

    // Using a timeout of 3 seconds to avoid unresponsive relays to block the loop forever.
    if let Ok(rx) = timeout(Duration::from_secs(3), relrequest).await {
        match rx {
            Ok(m) => {
                if m.is_empty() {
                    info!("No requested events found on relay {}", relay.0.to_string());
                }
                res = m
            }
            Err(_e) => println!("Error"),
        }
    }

    res
}

pub async fn send_relays_requests(client: &Client, filters: Filter) -> Vec<Vec<Event>> {
    let relays = client.relays().await;

    let relays_requests = relays.len();
    let mut requests: Vec<tokio::task::JoinHandle<Vec<Event>>> =
        Vec::with_capacity(relays_requests);
    let mut answers_requests = Vec::with_capacity(relays_requests);

    for relay in relays.into_iter() {
        info!("Requesting to relay : {}", relay.0.as_str());
        // Spawn futures and join them at the end
        requests.push(tokio::spawn(requests_relay(
            client.clone(),
            relay.clone(),
            filters.clone(),
        )));
    }

    // Get answers from relay
    for req in requests {
        answers_requests.push(req.await.unwrap());
    }

    answers_requests
}

pub async fn get_events_of_mostro(
    relay: &Relay,
    filters: Vec<Filter>,
    client: Client,
) -> Result<Vec<Event>, Error> {
    let mut events: Vec<Event> = Vec::new();

    // Subscribe
    info!(
        "Subscribing for all mostro orders to relay : {}",
        relay.url().to_string()
    );
    let id = SubscriptionId::new(Uuid::new_v4().to_string());
    let msg = ClientMessage::req(id.clone(), filters.clone());

    info!("Message sent : {:?}", msg);

    // Send msg to relay
    relay.send_msg(msg.clone())?;

    // Wait notification from relays
    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotification::Message { message, .. } = notification {
            match message {
                RelayMessage::Event {
                    subscription_id,
                    event,
                } => {
                    if subscription_id == id {
                        events.push(event.as_ref().clone());
                    }
                }
                RelayMessage::EndOfStoredEvents(subscription_id) => {
                    if subscription_id == id {
                        break;
                    }
                }
                _ => (),
            };
        }
    }

    // Unsubscribe
    relay.send_msg(ClientMessage::close(id))?;

    Ok(events)
}

pub async fn get_direct_messages(
    client: &Client,
    my_key: &Keys,
    since: i64,
    from_user: bool,
) -> Vec<(String, String, u64)> {
    // We use a fake timestamp to thwart time-analysis attacks
    let fake_since = 2880;
    let fake_since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(fake_since))
        .unwrap()
        .timestamp() as u64;

    let fake_timestamp = Timestamp::from(fake_since_time);
    let filters = if from_user {
        let since_time = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(since))
            .unwrap()
            .timestamp() as u64;
        let timestamp = Timestamp::from(since_time);
        Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .pubkey(my_key.public_key())
            .since(timestamp)
    } else {
        Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(my_key.public_key())
            .since(fake_timestamp)
    };

    info!("Request events with event kind : {:?} ", filters.kinds);

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, filters).await;

    // Buffer vector for direct messages
    let mut direct_messages: Vec<(String, String, u64)> = Vec::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut id_list = Vec::<EventId>::new();

    for dms in mostro_req.iter() {
        for dm in dms {
            if !id_list.contains(&dm.id) {
                id_list.push(dm.id);
                let created_at: Timestamp;
                let content: String;
                if from_user {
                    let ck = ConversationKey::derive(my_key.secret_key(), &dm.pubkey);
                    let b64decoded_content =
                        match general_purpose::STANDARD.decode(dm.content.as_bytes()) {
                            Ok(b64decoded_content) => b64decoded_content,
                            Err(_) => {
                                continue;
                            }
                        };
                    // Decrypt
                    let unencrypted_content = decrypt_to_bytes(&ck, b64decoded_content).unwrap();
                    content = String::from_utf8(unencrypted_content).expect("Found invalid UTF-8");
                    created_at = dm.created_at;
                } else {
                    let unwrapped_gift = match unwrap_gift_wrap(Some(my_key), None, None, dm) {
                        Ok(u) => u,
                        Err(_) => {
                            continue;
                        }
                    };
                    content = unwrapped_gift.rumor.content;
                    created_at = unwrapped_gift.rumor.created_at;
                }
                // Here we discard messages older than the real since parameter
                let since_time = chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(since))
                    .unwrap()
                    .timestamp() as u64;
                if created_at.as_u64() < since_time {
                    continue;
                }
                let date = DateTime::from_timestamp(created_at.as_u64() as i64, 0);

                let human_date = date.unwrap().format("%H:%M:%S date - %d/%m/%Y").to_string();
                direct_messages.push((content, human_date, created_at.as_u64()));
            }
        }
    }
    // Return element sorted by second tuple element ( Timestamp )
    direct_messages.sort_by(|a, b| a.2.cmp(&b.2));

    direct_messages
}

pub async fn get_orders_list(
    pubkey: PublicKey,
    status: Status,
    currency: Option<String>,
    kind: Option<MostroKind>,
    client: &Client,
) -> Result<Vec<SmallOrder>> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap()
        .timestamp() as u64;

    let timestamp = Timestamp::from(since_time);

    let filter = Filter::new()
        .author(pubkey)
        .limit(50)
        .since(timestamp)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), vec!["order"])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filter.authors, filter.kinds
    );

    // Extracted Orders List
    let mut complete_events_list = Vec::<SmallOrder>::new();
    let mut requested_orders_list = Vec::<SmallOrder>::new();

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, filter).await;
    // Scan events to extract all orders
    for orders_row in mostro_req.iter() {
        for ord in orders_row {
            let order = order_from_tags(ord.tags.clone());

            if order.is_err() {
                error!("{order:?}");
                continue;
            }
            let mut order = order?;

            info!("Found Order id : {:?}", order.id.unwrap());

            if order.id.is_none() {
                info!("Order ID is none");
                continue;
            }

            if order.kind.is_none() {
                info!("Order kind is none");
                continue;
            }

            if order.status.is_none() {
                info!("Order status is none");
                continue;
            }

            // Get created at field from Nostr event
            order.created_at = Some(ord.created_at.as_u64() as i64);

            complete_events_list.push(order.clone());

            if order.status.ne(&Some(status)) {
                continue;
            }

            if currency.is_some() && order.fiat_code.ne(&currency.clone().unwrap()) {
                continue;
            }

            if kind.is_some() && order.kind.ne(&kind) {
                continue;
            }
            // Add just requested orders requested by filtering
            requested_orders_list.push(order);
        }
    }

    // Order all element ( orders ) received to filter - discard disaligned messages
    // if an order has an older message with the state we received is discarded for the latest one
    requested_orders_list.retain(|keep| {
        !complete_events_list
            .iter()
            .any(|x| x.id == keep.id && x.created_at > keep.created_at)
    });
    // Sort by id to remove duplicates
    requested_orders_list.sort_by(|a, b| b.id.cmp(&a.id));
    requested_orders_list.dedup_by(|a, b| a.id == b.id);

    // Finally sort list by creation time
    requested_orders_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(requested_orders_list)
}

pub async fn get_disputes_list(pubkey: PublicKey, client: &Client) -> Result<Vec<Dispute>> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap()
        .timestamp() as u64;

    let timestamp = Timestamp::from(since_time);

    let filter = Filter::new()
        .author(pubkey)
        .limit(50)
        .since(timestamp)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), vec!["dispute"])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    // Extracted Orders List
    let mut disputes_list = Vec::<Dispute>::new();

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, filter).await;
    // Scan events to extract all disputes
    for disputes_row in mostro_req.iter() {
        for d in disputes_row {
            let dispute = dispute_from_tags(d.tags.clone());

            if dispute.is_err() {
                error!("{dispute:?}");
                continue;
            }
            let mut dispute = dispute?;

            info!("Found Dispute id : {:?}", dispute.id);

            // Get created at field from Nostr event
            dispute.created_at = d.created_at.as_u64() as i64;
            disputes_list.push(dispute);
        }
    }

    let buffer_dispute_list = disputes_list.clone();
    // Order all element ( orders ) received to filter - discard disaligned messages
    // if an order has an older message with the state we received is discarded for the latest one
    disputes_list.retain(|keep| {
        !buffer_dispute_list
            .iter()
            .any(|x| x.id == keep.id && x.created_at > keep.created_at)
    });

    // Sort by id to remove duplicates
    disputes_list.sort_by(|a, b| b.id.cmp(&a.id));
    disputes_list.dedup_by(|a, b| a.id == b.id);

    // Finally sort list by creation time
    disputes_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(disputes_list)
}

/// Uppercase first letter of a string.
pub fn uppercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn get_mcli_path() -> String {
    let home_dir = dirs::home_dir().expect("Couldn't get home directory");
    let mcli_path = format!("{}/.mcli", home_dir.display());
    if !Path::new(&mcli_path).exists() {
        fs::create_dir(&mcli_path).expect("Couldn't create mostro-cli directory in HOME");
        println!("Directory {} created.", mcli_path);
    }

    mcli_path
}

pub fn sign_content(content: Content, keys: &Keys) -> Result<Signature> {
    // content should be sha256 hashed
    let json: Value = json!(content);
    let content_str: String = json.to_string();
    let hash: Sha256Hash = Sha256Hash::hash(content_str.as_bytes());
    let hash = hash.to_byte_array();
    let message: BitcoinMessage = BitcoinMessage::from_digest(hash);
    let sig = keys.sign_schnorr(&message);

    Ok(sig)
}
