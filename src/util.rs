use crate::nip33::{dispute_from_tags, order_from_tags};

use anyhow::{Error, Result};
use bitcoin_hashes::sha256::Hash as Sha256Hash;
use chrono::NaiveDateTime;
use dotenvy::var;
use log::{error, info};
use mostro_core::dispute::{Dispute, Status as DisputeStatus};
use mostro_core::message::{Content, Message};
use mostro_core::order::Kind as MostroKind;
use mostro_core::order::{SmallOrder, Status};
use mostro_core::NOSTR_REPLACEABLE_EVENT_KIND;
use nostr_sdk::prelude::*;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

pub fn get_keys() -> Result<Keys> {
    // nostr private key
    let nsec1privkey = var("NSEC_PRIVKEY").expect("$NSEC_PRIVKEY env var needs to be set");
    let my_keys = Keys::from_sk_str(&nsec1privkey)?;

    Ok(my_keys)
}

pub async fn send_dm(
    client: &Client,
    sender_keys: &Keys,
    receiver_pubkey: &XOnlyPublicKey,
    content: String,
    _wait_for_connection: Option<bool>,
) -> Result<()> {
    let event =
        EventBuilder::new_encrypted_direct_msg(sender_keys, *receiver_pubkey, content, None)?
            .to_event(sender_keys)?;
    info!("Sending event: {event:#?}");
    // FIX: The client by default is created with wait_for_send = false, we probably don't need this
    // This will update relay send event to wait for tranmission.
    // if let Some(_wait_mes) = wait_for_connection {
    //     let opts = Options::new().wait_for_send(false);
    //     Client::new_with_opts(sender_keys, opts);
    // }
    let msg = ClientMessage::new_event(event);
    client.send_msg(msg).await?;

    Ok(())
}

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = crate::util::get_keys()?;

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    // Create new client
    let opts = Options::new().wait_for_connection(false);
    let client = Client::with_opts(&my_keys, opts);
    // Add relays
    for r in relays.into_iter() {
        client.add_relay(r, None).await?;
    }
    // Connect to relays and keep connection alive
    client.connect().await;

    Ok(client)
}

pub async fn send_order_id_cmd(
    client: &Client,
    my_key: &Keys,
    mostro_pubkey: XOnlyPublicKey,
    message: String,
    wait_for_dm_ans: bool,
) -> Result<()> {
    info!("Sending message: {message:#?}");
    // Send dm to mostro pub id
    send_dm(client, my_key, &mostro_pubkey, message, Some(false)).await?;

    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if wait_for_dm_ans {
            let dm = get_direct_messages(client, mostro_pubkey, my_key, 1).await;

            for el in dm.iter() {
                match Message::from_json(&el.0) {
                    Ok(m) => {
                        if let Some(Content::PaymentRequest(ord, inv)) =
                            &m.get_inner_message_kind().content
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
                        println!("Mostro sent you this message -->  {}", el.0);
                        println!();
                    }
                }
            }
            break;
        } else if let RelayPoolNotification::Message(
            _,
            RelayMessage::Ok {
                event_id: _,
                status: _,
                message: _,
            },
        ) = notification
        {
            println!(
                "Message correctly sent to Mostro! Check messages with getdm or listorders command"
            );
            break;
        }
    }
    Ok(())
}

pub async fn requests_relay(client: Client, relay: (Url, Relay), filters: Filter) -> Vec<Event> {
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
    let msg = ClientMessage::new_req(id.clone(), filters.clone());

    info!("Message sent : {:?}", msg);

    // Send msg to relay
    relay.send_msg(msg.clone(), None).await?;

    // Wait notification from relays
    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotification::Message(_, msg) = notification {
            match msg {
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
    relay.send_msg(ClientMessage::close(id), None).await?;

    Ok(events)
}

pub async fn get_direct_messages(
    client: &Client,
    mostro_pubkey: XOnlyPublicKey,
    my_key: &Keys,
    since: i64,
) -> Vec<(String, String)> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(since))
        .unwrap()
        .timestamp() as u64;

    let timestamp = Timestamp::from(since_time);
    let filters = Filter::new()
        .author(mostro_pubkey.to_string())
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(my_key.public_key())
        .since(timestamp);

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filters.authors, filters.kinds
    );

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, filters).await;

    // Buffer vector for direct messages
    let mut direct_messages: Vec<(String, String)> = Vec::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut id_list = Vec::<Sha256Hash>::new();

    for dms in mostro_req.iter() {
        for dm in dms {
            if !id_list.contains(&dm.id.inner()) {
                id_list.push(dm.id.inner());
                let date = NaiveDateTime::from_timestamp_opt(dm.created_at.as_i64(), 0);

                let human_date = date.unwrap().format("%H:%M date - %d/%m/%Y").to_string();

                let message = decrypt(
                    &my_key.secret_key().unwrap(),
                    &dm.pubkey,
                    dm.content.clone(),
                );
                direct_messages.push(((message.unwrap()), (human_date)));
            }
        }
    }
    // Return element sorted by second tuple element ( Timestamp )
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));

    direct_messages
}

pub async fn get_orders_list(
    pubkey: XOnlyPublicKey,
    status: Status,
    currency: Option<String>,
    kind: Option<MostroKind>,
    client: &Client,
) -> Result<Vec<SmallOrder>> {
    let generic_filter = Filter::new()
        .author(pubkey.to_string())
        .custom_tag(Alphabet::Z, vec!["order"])
        .custom_tag(Alphabet::S, vec![status.to_string()])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    let mut exec_filter = generic_filter;

    if let Some(c) = currency {
        exec_filter = exec_filter
            .clone()
            .custom_tag(Alphabet::F, vec![c.to_string()]);
    }

    if let Some(k) = kind {
        exec_filter = exec_filter
            .clone()
            .custom_tag(Alphabet::K, vec![k.to_string()]);
    }

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        exec_filter.authors, exec_filter.kinds
    );

    // Extracted Orders List
    let mut orders_list = Vec::<SmallOrder>::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut id_list = Vec::<Uuid>::new();

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, exec_filter).await;
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
            order.created_at = Some(ord.created_at.as_i64());
            // Add only in case id of order is not present in the list (avoid duplicate)
            if !id_list.contains(&order.id.unwrap()) {
                id_list.push(order.id.unwrap());
                orders_list.push(order);
            }
        }
    }
    // Return element sorted by second tuple element ( Timestamp )
    orders_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(orders_list)
}

pub async fn get_disputes_list(pubkey: XOnlyPublicKey, client: &Client) -> Result<Vec<Dispute>> {
    let generic_filter = Filter::new()
        .author(pubkey.to_string())
        .custom_tag(Alphabet::Z, vec!["dispute"])
        .custom_tag(Alphabet::S, vec![DisputeStatus::Initiated.to_string()])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    let exec_filter = generic_filter;

    // Extracted Orders List
    let mut disputes_list = Vec::<Dispute>::new();

    // Vector for single dispute id check - maybe multiple relay could send the same dispute id? Check unique one...
    let mut id_list = Vec::<Uuid>::new();

    // Send all requests to relays
    let mostro_req = send_relays_requests(client, exec_filter).await;
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
            dispute.created_at = d.created_at.as_i64();

            // Add only in case id of dispute is not present in the list (avoid duplicate)
            if !id_list.contains(&dispute.id) {
                id_list.push(dispute.id);
                disputes_list.push(dispute);
            }
        }
    }

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
