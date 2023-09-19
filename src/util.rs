use anyhow::{Error, Result};
use bitcoin_hashes::sha256::Hash as Sha256Hash;
use chrono::NaiveDateTime;
use dotenvy::var;
use log::{error, info};
use mostro_core::order::NewOrder;
use mostro_core::Kind as MostroKind;
use mostro_core::Message as MostroMessage;
use mostro_core::{Content, Status, NOSTR_REPLACEABLE_EVENT_KIND};
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
    let event = EventBuilder::new_encrypted_direct_msg(sender_keys, *receiver_pubkey, content)?
        .to_event(sender_keys)?;
    info!("Sending event: {event:#?}");
    // FIX: The client by default is created with wait_for_send = false, we probably don't need this
    // This will update relay send event to wait for tranmission.
    // if let Some(_wait_mes) = wait_for_connection {
    //     let opts = Options::new().wait_for_send(false);
    //     Client::new_with_opts(sender_keys, opts);
    // }
    client.send_event(event).await?;

    Ok(())
}

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = crate::util::get_keys()?;

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    // Create new client
    let opts = Options::new().wait_for_connection(false);
    let client = Client::new_with_opts(&my_keys, opts);
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
    // Send dm to mostro pub id
    send_dm(client, my_key, &mostro_pubkey, message, Some(false)).await?;

    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if wait_for_dm_ans {
            let dm = get_direct_messages(client, mostro_pubkey, my_key, 1).await;

            for el in dm.iter() {
                match MostroMessage::from_json(&el.0) {
                    Ok(m) => {
                        if let Some(Content::PaymentRequest(ord, inv)) = m.content {
                            println!("NEW MESSAGE:");
                            println!(
                                "Mostro sent you this hold invoice for order id: {}",
                                ord.unwrap().id.unwrap()
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
    relay.send_msg(msg.clone(), false).await?;

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
    relay.send_msg(ClientMessage::close(id), false).await?;

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
        .author(mostro_pubkey)
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(my_key.public_key())
        .since(timestamp);

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filters.authors.as_ref().unwrap(),
        filters.kinds.as_ref().unwrap()
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
                let message = decrypt(
                    &my_key.secret_key().unwrap(),
                    &dm.pubkey,
                    dm.content.clone(),
                );
                direct_messages.push(((message.unwrap()), (date.unwrap().to_string())));
            }
        }
    }
    direct_messages
}

pub async fn get_orders_list(
    pubkey: XOnlyPublicKey,
    status: Option<Status>,
    currency: Option<String>,
    kind: Option<MostroKind>,
    client: &Client,
) -> Result<Vec<NewOrder>> {
    let filters = Filter::new()
        .author(pubkey)
        .kind(Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filters.authors.as_ref().unwrap(),
        filters.kinds.as_ref().unwrap()
    );

    // Extracted Orders List
    let mut orderslist = Vec::<NewOrder>::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut idlist = Vec::<Uuid>::new();

    //Send all requests to relays
    let mostro_req = send_relays_requests(client, filters).await;

    //Scan events to extract all orders
    for ordersrow in mostro_req.iter() {
        for ord in ordersrow {
            let order = NewOrder::from_json(&ord.content);

            if order.is_err() {
                error!("{order:?}");
                continue;
            }
            let order = order?;

            info!("Found Order id : {:?}", order.id.unwrap());

            //Match order status
            if let Some(ref st) = status {
                //If order is yet present go on...
                if idlist.contains(&order.id.unwrap()) || *st != order.status {
                    info!("Found same id order {}", order.id.unwrap());
                    continue;
                }
            }

            //Match currency
            if let Some(ref curr) = currency {
                if *curr != order.fiat_code {
                    info!(
                        "Not requested currency offer - you requested this currency {:?}",
                        currency
                    );
                    continue;
                }
            }

            //Match order kind
            if let Some(ref reqkind) = kind {
                if *reqkind != order.kind {
                    info!("Not requested kind - you requested {:?} offers", kind);
                    continue;
                }
            }

            idlist.push(order.id.unwrap());
            orderslist.push(order);
        }
    }
    Ok(orderslist)
}
