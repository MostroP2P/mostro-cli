use crate::types::Action;
use crate::types::Content;
use crate::types::Kind as Orderkind;
use crate::types::Message;
use crate::types::Order;
use crate::types::Status;
use anyhow::{Error, Result};
use chrono::NaiveDateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use dotenvy::var;
use nostr_sdk::prelude::*;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;
use log::{error, info};


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
    if let Some(_wait_mes) = wait_for_connection{
        let opts = Options::new().wait_for_connection(true);
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
        "wss://nostr.itssilvestre.com",
        "wss://nostr.drss.io",
        "wss://nostr.zebedee.cloud",
        "wss://public.nostr.swissrouting.com",
        "wss://nostr.slothy.win",
        "wss://nostr.rewardsbunny.com",
        "wss://relay.nostropolis.xyz/websocket",
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
        "wss://nostr.itssilvestre.com",
        "wss://relay.nostr.nu",
        "wss://nostr.easydns.ca",
        "wss://no-str.org",
        "wss://nostrical.com",
        // "wss://student.chadpolytechnic.com",
    ];

    // Add relays
    for r in relays.into_iter() {
        let opts = Options::new().wait_for_connection(true).wait_for_send(true);
        client.update_opts(opts);
        client.add_relay(r, None).await?;
    }

    // Connect to relays and keep connection alive
    
    client.connect().await;

    Ok(client)
}

pub async fn take_order_id(client: &Client, my_key : &Keys, mostro_pubkey : XOnlyPublicKey, id : &i64 , invoice : &String) -> Result<()> {
    
    let takesell_message = Message::new(
        0,
        *id, 
        Action::TakeSell,
        Content::PaymentRequest(invoice.to_string()),
        
    ).as_json().unwrap();

    // Send dm to mostro pub id
    send_dm(client, my_key, &mostro_pubkey, takesell_message , Some(true)).await?;

    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotification::Message(_, RelayMessage::Ok { event_id:_, status:_,message:_ }) = notification {
                    break;
                }                
            };

    Ok(())
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
    relay.send_msg(msg.clone(), false).await?;

    // let mut notifications = rx.notifications();
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

pub async fn get_direct_messages(client : &Client, mostro_pubkey : XOnlyPublicKey, my_key : &Keys) -> Result<()>{

    let since_time = chrono::Utc::now();


    let filters = SubscriptionFilter::new()
        .author(mostro_pubkey)
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(my_key.public_key())
        .since(since_time.checked_sub_signed(chrono::Duration::seconds(5)).unwrap().timestamp() as u64);
    
    info!(
        "Request to mostro id : {:?} with event kind : {:?} ",
        filters.authors.as_ref().unwrap(),
        filters.kinds.as_ref().unwrap()
    );

    let relays = client.relays().await;

    // Collector of mostro orders on a specific relay
    let mut mostro_req: Vec<Vec<Event>> = vec![];

    for relay in relays.iter() {
        info!("Requesting to relay : {}", relay.0.as_str());

        let relrequest = get_events_of_mostro(relay.1, vec![filters.clone()], client);

        //Using a timeout of 5 seconds to avoid unresponsive relays to block the loop forever.
        if let Ok(rx) = timeout(Duration::from_secs(3), relrequest).await {
            match rx {
                Ok(m) => {
                    if m.is_empty() {
                        info!("No message found on relay {}", relay.0.to_string());
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

    for dms in mostro_req.iter(){
        for dm in dms{
            let message = decrypt(&my_key.secret_key().unwrap(), &dm.pubkey, dm.content.clone());
            println!("{}", message.unwrap());
        }
    }



    Ok(())

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

    let relays = client.relays().await;

    // Collector of mostro orders on a specific relay
    let mut mostro_req: Vec<Vec<Event>> = vec![];

    // Extracted Orders List
    let mut orderslist = Vec::<Order>::new();

    // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
    let mut idlist = Vec::<Uuid>::new();

    for relay in relays.iter() {
        info!("Requesting to relay : {}", relay.0.as_str());

        let relrequest = get_events_of_mostro(relay.1, vec![filters.clone()], client);

        //Using a timeout of 5 seconds to avoid unresponsive relays to block the loop forever.
        if let Ok(rx) = timeout(Duration::from_secs(3), relrequest).await {
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

pub fn print_orders_table(orders_table: Vec<Order>) -> Result<String> {
    let mut table = Table::new();

    //Table rows
    let mut rows: Vec<Row> = Vec::new();

    if orders_table.is_empty() {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![Cell::new("Sorry...")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center)]);

        // Single row for error
        let mut r = Row::new();

        r.add_cell(
            Cell::new("No offers found with requested parameters...")
                .fg(Color::Red)
                .set_alignment(CellAlignment::Center),
        );

        //Push single error row
        rows.push(r);
    } else {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![
                Cell::new("Buy/Sell")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Order Id")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Status")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Fiat Code")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Fiat Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Payment method")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Created")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
            ]);

        //Iterate to create table of orders
        for single_order in orders_table.into_iter() {
            let date =
                NaiveDateTime::from_timestamp_opt(single_order.created_at.unwrap_or(0) as i64, 0);

            let r = Row::from(vec![
                // Cell::new(single_order.kind.to_string()),
                match single_order.kind {
                    crate::types::Kind::Buy => Cell::new(single_order.kind.to_string())
                        .fg(Color::Green)
                        .set_alignment(CellAlignment::Center),
                    crate::types::Kind::Sell => Cell::new(single_order.kind.to_string())
                        .fg(Color::Red)
                        .set_alignment(CellAlignment::Center),
                },
                Cell::new(single_order.id.unwrap()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.status.to_string()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.amount.to_string()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.fiat_amount.to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(single_order.payment_method.to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(date.unwrap()),
            ]);
            rows.push(r);
        }
    }

    table.add_rows(rows);

    Ok(table.to_string())
}
