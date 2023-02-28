use crate::types::Kind as Orderkind;
use crate::types::Order;
use crate::types::Status;
use anyhow::{Error, Result};
use chrono::NaiveDateTime;
use dotenvy::var;
use log::{error, info};
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
    wait_for_connection: Option<bool>,
) -> Result<()> {
    let event = EventBuilder::new_encrypted_direct_msg(sender_keys, *receiver_pubkey, content)?
        .to_event(sender_keys)?;
    info!("Sending event: {event:#?}");
    // This will update relay send event to wait for tranmission.
    if let Some(_wait_mes) = wait_for_connection {
        let opts = Options::new().wait_for_connection(false);
        client.update_opts(opts);
    }
    client.send_event(event).await?;

    Ok(())
}

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = crate::util::get_keys()?;

    // Create new client
    let client = Client::new(&my_keys);

    let relays = vec![
        "wss://relay.nostr.vision",
        "wss://nostr.zebedee.cloud",
        "wss://public.nostr.swissrouting.com",
        "wss://nostr.slothy.win",
        "wss://nostr.rewardsbunny.com",
        "wss://nostr.supremestack.xyz",
        "wss://nostr.shawnyeager.net",
        "wss://relay.nostrmoto.xyz",
        "wss://nostr.roundrockbitcoiners.com",
        "wss://nostr.utxo.lol",
        "wss://nostr-relay.schnitzel.world",
        "wss://sg.qemura.xyz",
        "wss://nostr.digitalreformation.info",
        "wss://nostr-relay.usebitcoin.space",
        "wss://nostr.bch.ninja",
        "wss://nostr.massmux.com",
        "wss://nostr-pub1.southflorida.ninja",
        "wss://relay.nostr.nu",
        "wss://nostr.easydns.ca",
        "wss://nostrical.com",
        "wss://relay.damus.io",
    ];

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
) -> Result<()> {
    // Send dm to mostro pub id
    send_dm(client, my_key, &mostro_pubkey, message, Some(true)).await?;

    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotification::Message(
            _,
            RelayMessage::Ok {
                event_id: _,
                status: _,
                message: _,
            },
        ) = notification
        {
            println!("Message correctly sent to Mostro! Check messages with get-dm command");
            break;
        }
    }
    Ok(())
}

pub async fn requests_relay(
    client: Client,
    relay: (Url, Relay),
    filters: SubscriptionFilter,
) -> Vec<Event> {
    let relrequest = get_events_of_mostro(&relay.1, vec![filters.clone()], client);

    // Buffer vector
    let mut res: Vec<Event> = Vec::new();

    //Using a timeout of 3 seconds to avoid unresponsive relays to block the loop forever.
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
    } else {
        println!("Timeout on request from {}", relay.0);
    };
    res
}

pub async fn send_relays_requests(client: &Client, filters: SubscriptionFilter) -> Vec<Vec<Event>> {
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
    filters: Vec<SubscriptionFilter>,
    client: Client,
) -> Result<Vec<Event>, Error> {
    let mut events: Vec<Event> = Vec::new();

    let id = Uuid::new_v4();

    // Subscribe
    info!(
        "Subscribing for all mostro orders to relay : {}",
        relay.url().to_string()
    );
    let msg = ClientMessage::new_req(id.to_string(), filters.clone());

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
                    if subscription_id == id.to_string() {
                        events.push(event.as_ref().clone());
                    }
                }
                RelayMessage::EndOfStoredEvents { subscription_id } => {
                    if subscription_id == id.to_string() {
                        break;
                    }
                }
                _ => (),
            };
        }
    }

    // Unsubscribe
    relay
        .send_msg(ClientMessage::close(id.to_string()), false)
        .await?;

    Ok(events)
}

pub async fn get_direct_messages(
    client: &Client,
    mostro_pubkey: XOnlyPublicKey,
    my_key: &Keys,
    since: i64,
) -> Vec<(String, String)> {
    let since_time = chrono::Utc::now();

    let filters = SubscriptionFilter::new()
        .author(mostro_pubkey)
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(my_key.public_key())
        .since(
            since_time
                .checked_sub_signed(chrono::Duration::minutes(since))
                .unwrap()
                .timestamp() as u64,
        );

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
    let mut idlist = Vec::<Sha256Hash>::new();

    for dms in mostro_req.iter() {
        for dm in dms {
            if !idlist.contains(&dm.id) {
                idlist.push(dm.id);
                let date = NaiveDateTime::from_timestamp_opt(dm.created_at as i64, 0);
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
    kind: Option<Orderkind>,
    client: &Client,
) -> Result<Vec<Order>> {
    let filters = SubscriptionFilter::new()
        .author(pubkey)
        .kind(Kind::Custom(30000));

    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filters.authors.as_ref().unwrap(),
        filters.kinds.as_ref().unwrap()
    );

    // Extracted Orders List
    let mut orderslist = Vec::<Order>::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut idlist = Vec::<Uuid>::new();

    //Send all requests to relays
    let mostro_req = send_relays_requests(client, filters).await;

    //Scan events to extract all orders
    for ordersrow in mostro_req.iter() {
        for ord in ordersrow {
            let order = Order::from_json(&ord.content);

            if order.is_err() {
                error!("{order:?}");
                continue;
            }
            let order = order?;

            info!("Found Order id : {:?}", order.id.unwrap());

            //Match order status
            if let Some(st) = status {
                //If order is yet present go on...
                if idlist.contains(&order.id.unwrap()) || st != order.status {
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
            if let Some(reqkind) = kind {
                if reqkind != order.kind {
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
