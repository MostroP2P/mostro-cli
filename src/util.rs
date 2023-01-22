use crate::types::Order;
use anyhow::{Error, Result};
use chrono::NaiveDateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use dotenvy::var;
use log::{error, info};
use nostr::key::FromSkStr;
use nostr::key::XOnlyPublicKey;
use nostr::{ClientMessage, Event, Kind, RelayMessage, SubscriptionFilter};
use nostr_sdk::{Client, Relay, RelayPoolNotifications};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

pub fn get_keys() -> Result<nostr::Keys> {
    // nostr private key
    let nsec1privkey = var("NSEC_PRIVKEY").expect("$NSEC_PRIVKEY env var needs to be set");
    let my_keys = nostr::key::Keys::from_sk_str(&nsec1privkey)?;
    Ok(my_keys)
}

pub async fn connect_nostr() -> Result<nostr_sdk::Client> {
    let my_keys = crate::util::get_keys()?;

    // Create new client
    let client = nostr_sdk::Client::new(&my_keys);

    let relays = vec![
        "wss://nostr.p2sh.co",
        "wss://relay.nostr.vision",
        "wss://nostr.itssilvestre.com",
        "wss://nostr.drss.io",
        "wss://relay.nostr.info",
        "wss://relay.nostr.pro",
        "wss://nostr.zebedee.cloud",
        "wss://nostr.fmt.wiz.biz",
        "wss://public.nostr.swissrouting.com",
        "wss://nostr.slothy.win",
        "wss://nostr.rewardsbunny.com",
        "wss://relay.nostropolis.xyz/websocket",
        "wss://nostr.supremestack.xyz",
        "wss://nostr.orba.ca",
        "wss://nostr.mom",
        "wss://nostr.yael.at",
        "wss://nostr-relay.derekross.me",
        "wss://nostr.shawnyeager.net",
        "wss://nostr.jiashanlu.synology.me",
        "wss://relay.ryzizub.com",
        "wss://relay.nostrmoto.xyz",
        "wss://jiggytom.ddns.net",
        "wss://nostr.sectiontwo.org",
        "wss://nostr.roundrockbitcoiners.com",
        "wss://nostr.utxo.lol",
        "wss://relay.nostrid.com",
        "wss://nostr1.starbackr.me",
        "wss://nostr-relay.schnitzel.world",
        "wss://sg.qemura.xyz",
        "wss://nostr.digitalreformation.info",
        "wss://nostr-relay.usebitcoin.space",
        "wss://nostr.bch.ninja",
        "wss://nostr.demovement.net",
        "wss://nostr.massmux.com",
        "wss://relay.nostr.bg",
        "wss://nostr-pub1.southflorida.ninja",
        "wss://nostr.itssilvestre.com",
        "wss://nostr-1.nbo.angani.co",
        "wss://relay.nostr.nu",
        "wss://nostr.easydns.ca",
        "wss://no-str.org",
        "wss://nostr.milou.lol",
        "wss://nostrical.com",
        "wss://pow32.nostr.land",
        "wss://student.chadpolytechnic.com",
    ];

    // Add relays
    for r in relays {
        client.add_relay(r, None).await?;
    }

    // Connect to relays and keep connection alive
    client.connect().await?;

    Ok(client)
}

pub async fn get_events_of_mostro(
    relay: &Relay,
    filters: Vec<SubscriptionFilter>,
    client: &Client,
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

    //Send msg to relay
    relay.send_msg(msg.clone()).await?;

    // let mut notifications = rx.notifications();
    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotifications::ReceivedMessage(msg) = notification {
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
    relay.send_msg(ClientMessage::close(id.to_string())).await?;

    Ok(events)
}

pub async fn get_orders_list(
    pubkey: XOnlyPublicKey,
    status: String,
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

    let relays = client.relays().await;

    // Collector of mostro orders on a specific relay
    let mut mostro_req: Vec<Vec<Event>> = vec![];

    // Extracted Orders List
    let mut orderslist = Vec::<Order>::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut idlist = Vec::<i64>::new();

    for relay in relays.iter() {
        info!("Requesting to relay : {}", relay.0.as_str());

        let relrequest = get_events_of_mostro(relay.1, vec![filters.clone()], client);

        //Using a timeout of 5 seconds to avoid unresponsive relays to block the loop forever.
        if let Ok(rx) = timeout(Duration::from_secs(5), relrequest).await {
            match rx {
                Ok(m) => {
                    if m.is_empty() {
                        info!("No order found on relay {}", relay.0.to_string());
                    } else {
                        mostro_req.push(m)
                    }
                }
                Err(_e) => println!("Error"),
            }
        } else {
            println!("Timeout on request from {}", relay.0);
        };
    }

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

            //Just add selected status
            if order.status.to_string() == status {
                //If order is yet present go on...
                if idlist.contains(&order.id.unwrap()) {
                    info!("Found same id order {}", order.id.unwrap());
                    continue;
                };
                idlist.push(order.id.unwrap());
                orderslist.push(order);
            }
        }
    }

    Ok(orderslist)
}

pub fn print_orders_table(orderstable: Vec<Order>) -> Result<String> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(160)
        .set_header(vec![
            Cell::new("Buy/Sell").add_attribute(Attribute::Bold),
            Cell::new("Order Id").add_attribute(Attribute::Bold),
            Cell::new("Status").add_attribute(Attribute::Bold),
            Cell::new("Amount").add_attribute(Attribute::Bold),
            Cell::new("Fiat Code").add_attribute(Attribute::Bold),
            Cell::new("Fiat Amount").add_attribute(Attribute::Bold),
            Cell::new("Payment method").add_attribute(Attribute::Bold),
            Cell::new("Created").add_attribute(Attribute::Bold),
        ]);

    //Table rows
    let mut rows: Vec<Row> = Vec::new();

    //Iterate to create table of orders
    for singleorder in orderstable.into_iter() {
        let date = NaiveDateTime::from_timestamp_opt(singleorder.created_at.unwrap() as i64, 0);

        let r = Row::from(vec![
            Cell::new(singleorder.kind.to_string()),
            Cell::new(singleorder.id.unwrap()),
            Cell::new(singleorder.status.to_string()),
            Cell::new(singleorder.amount.to_string()),
            Cell::new(singleorder.fiat_code.to_string()),
            Cell::new(singleorder.fiat_amount.to_string()),
            Cell::new(singleorder.payment_method.to_string()),
            Cell::new(date.unwrap()),
        ]);
        rows.push(r);
    }

    table.add_rows(rows);

    //println!("{table}");

    Ok(table.to_string())
}
