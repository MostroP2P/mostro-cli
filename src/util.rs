use crate::nip33::{dispute_from_tags, order_from_tags};

use anyhow::{Error, Result};
use bitcoin_hashes::sha256::Hash as Sha256Hash;
use chrono::NaiveDateTime;
use dotenvy::var;
use log::{error, info};
use mostro_core::dispute::Dispute;
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
    let my_keys = Keys::parse(nsec1privkey)?;

    Ok(my_keys)
}

pub async fn send_dm(
    client: &Client,
    sender_keys: &Keys,
    receiver_pubkey: &PublicKey,
    content: String,
    _wait_for_connection: Option<bool>,
) -> Result<()> {
    let event = EventBuilder::encrypted_direct_msg(sender_keys, *receiver_pubkey, content, None)?
        .to_event(sender_keys)?;
    info!("Sending event: {event:#?}");
    // FIX: The client by default is created with wait_for_send = false, we probably don't need this
    // This will update relay send event to wait for tranmission.
    // if let Some(_wait_mes) = wait_for_connection {
    //     let opts = Options::new().wait_for_send(false);
    //     Client::new_with_opts(sender_keys, opts);
    // }
    let msg = ClientMessage::event(event);
    client.send_msg(msg).await?;

    Ok(())
}

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = crate::util::get_keys()?;

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    // Create new client
    let opts = Options::new().wait_for_send(false);
    let client = Client::with_opts(&my_keys, opts);
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
    my_key: &Keys,
    mostro_pubkey: PublicKey,
    message: String,
    wait_for_dm_ans: bool,
) -> Result<()> {
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
    let msg = ClientMessage::req(id.clone(), filters.clone());

    info!("Message sent : {:?}", msg);

    // Send msg to relay
    relay.send_msg(msg.clone(), RelaySendOptions::new()).await?;

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
    relay
        .send_msg(ClientMessage::close(id), RelaySendOptions::new())
        .await?;

    Ok(events)
}

pub async fn get_direct_messages(
    client: &Client,
    mostro_pubkey: PublicKey,
    my_key: &Keys,
    since: i64,
) -> Vec<(String, String)> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(since))
        .unwrap()
        .timestamp() as u64;

    let timestamp = Timestamp::from(since_time);
    let filters = Filter::new()
        .author(mostro_pubkey)
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

                let message =
                    nip04::decrypt(my_key.secret_key().unwrap(), &dm.pubkey, dm.content.clone());
                direct_messages.push(((message.unwrap()), (human_date)));
            }
        }
    }
    // Return element sorted by second tuple element ( Timestamp )
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));

    direct_messages
}

pub async fn get_orders_list(
    pubkey: PublicKey,
    status: Status,
    currency: Option<String>,
    kind: Option<MostroKind>,
    client: &Client,
) -> Result<Vec<SmallOrder>> {
    let filter = Filter::new()
        .author(pubkey)
        .limit(50)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), vec!["order"])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filter.authors, filter.kinds
    );

    // Extracted Orders List
    let mut orders_list = Vec::<SmallOrder>::new();
    let mut remove_list = Vec::<usize>::new();

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
            order.created_at = Some(ord.created_at.as_i64());

            orders_list.push(order);
        }
    }

    // Order all element ( orders ) received to filter
    orders_list.sort_by(|a, b| b.id.cmp(&a.id));

    for el in orders_list.iter().enumerate() {
        // We check if adjacent elements have same id, in this case we
        // prepare to remove oldest ones
        if el.0 < orders_list.len() - 1 {
            if el.1.id.unwrap() == orders_list[el.0 + 1].id.unwrap() && el.0 < orders_list.len() - 1
            {
                println!("item duplicated {:?}", el.0);
                if el.1.created_at <= orders_list[el.0 + 1].created_at {
                    remove_list.push(el.0)
                } else {
                    remove_list.push(el.0 + 1)
                }
            }
        }

        if el.1.status.ne(&Some(status)) && !remove_list.contains(&el.0) {
            remove_list.push(el.0)
        }

        if currency.is_some()
            && el.1.fiat_code.ne(&currency.clone().unwrap())
            && !remove_list.contains(&el.0)
        {
            remove_list.push(el.0)
        }

        if kind.is_some() && el.1.kind.ne(&kind) && !remove_list.contains(&el.0) {
            remove_list.push(el.0)
        }
    }

    // Remove duplicate and not meaningful events.
    let len = remove_list.len();
    for n in (0..len).rev() {
        orders_list.remove(remove_list[n]);
    }

    // Return element sorted by second tuple element ( Timestamp )
    orders_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(orders_list)
}

pub async fn get_disputes_list(pubkey: PublicKey, client: &Client) -> Result<Vec<Dispute>> {
    let filter = Filter::new()
        .author(pubkey)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), vec!["dispute"])
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    // Extracted Orders List
    let mut disputes_list = Vec::<Dispute>::new();
    let mut remove_list = Vec::<usize>::new();

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
            dispute.created_at = d.created_at.as_i64();
            disputes_list.push(dispute);
        }
    }
    // Order all element ( orders ) received to filter
    disputes_list.sort_by(|a, b| b.id.cmp(&a.id));

    for el in disputes_list.iter().enumerate() {
        // We check if adjacent elements have same id, in this case we
        // prepare to remove oldest ones
        if el.1.id == disputes_list[el.0 + 1].id {
            if el.1.created_at < disputes_list[el.0 + 1].created_at {
                remove_list.push(el.0)
            } else {
                remove_list.push(el.0 + 1)
            }
        }
    }

    // Remove duplicate and not meaningful events.
    for remove_el in remove_list {
        disputes_list.remove(remove_el);
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
