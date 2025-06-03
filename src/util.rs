use crate::db::{connect, Order, User};
use crate::nip33::{dispute_from_tags, order_from_tags};
use anyhow::{Error, Result};
use base64::engine::general_purpose;
use base64::Engine;
use dotenvy::var;
use log::{error, info};
use mostro_core::prelude::*;
use nip44::v2::{decrypt_to_bytes, encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use std::time::Duration;
use std::{fs, path::Path};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    SmallOrder(SmallOrder),
    Dispute(Dispute), // Assuming you have a Dispute struct
    MessageTuple((Message, u64)),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ListKind {
    Orders,
    Disputes,
    DirectMessages,
}

pub async fn save_order(
    order: SmallOrder,
    trade_keys: &Keys,
    request_id: u64,
    trade_index: i64,
) -> Result<()> {
    let pool = connect().await?;
    if let Ok(order) = Order::new(&pool, order, trade_keys, Some(request_id as i64)).await {
        if let Some(order_id) = order.id {
            println!("Order {} created", order_id);
        } else {
            println!("Warning: The newly created order has no ID.");
        }
        // Update last trade index to be used in next trade
        match User::get(&pool).await {
            Ok(mut user) => {
                user.set_last_trade_index(trade_index);
                if let Err(e) = user.save(&pool).await {
                    println!("Failed to update user: {}", e);
                }
            }
            Err(e) => println!("Failed to get user: {}", e),
        }
    }
    Ok(())
}

/// Wait for incoming gift wraps or events coming in
pub async fn wait_for_dm(
    client: &Client,
    trade_keys: &Keys,
    request_id: u64,
    trade_index: i64,
    mut order: Option<Order>,
) -> anyhow::Result<()> {
    let mut notifications = client.notifications();

    match tokio::time::timeout(Duration::from_secs(10), async move {
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Event { event, .. } = notification {
                if event.kind == nostr_sdk::Kind::GiftWrap {
                let gift = nip59::extract_rumor(trade_keys, &event).await.unwrap();
                let (message, _): (Message, Option<String>) = serde_json::from_str(&gift.rumor.content).unwrap();
                let message = message.get_inner_message_kind();
                if message.request_id == Some(request_id) {
                    match message.action {
                        Action::NewOrder => {
                            if let Some(Payload::Order(order)) = message.payload.as_ref() {
                                save_order(order.clone(), trade_keys, request_id, trade_index).await.map_err(|_| ())?;
                                return Ok(());
                            }
                        }
                        // this is the case where the buyer adds an invoice to a takesell order
                        Action::WaitingSellerToPay => {
                            println!("Now we should wait for the seller to pay the invoice");
                            if let Some(mut order) = order.take() {
                                let pool = connect().await.map_err(|_| ())?;
                                match order
                                .set_status(Status::WaitingPayment.to_string())
                                .save(&pool)
                                .await
                                {
                                    Ok(_) => println!("Order status updated"),
                                    Err(e) => println!("Failed to update order status: {}", e),
                                }
                            }
                        }
                        // this is the case where the buyer adds an invoice to a takesell order
                        Action::AddInvoice => {
                            if let Some(Payload::Order(order)) = &message.payload {
                                println!(
                                    "Please add a lightning invoice with amount of {}",
                                    order.amount
                                );
                                return Ok(());
                            }
                        }
                        // this is the case where the buyer pays the invoice coming from a takebuy
                        Action::PayInvoice => {
                            if let Some(Payload::PaymentRequest(order, invoice, _)) = &message.payload {
                                println!(
                                    "Mostro sent you this hold invoice for order id: {}",
                                    order
                                        .as_ref()
                                        .and_then(|o| o.id)
                                        .map_or("unknown".to_string(), |id| id.to_string())
                                );
                                println!();
                                println!("Pay this invoice to continue -->  {}", invoice);
                                println!();
                                if let Some(order) = order {
                                    let store_order = order.clone();
                                    save_order(store_order, trade_keys, request_id, trade_index).await.map_err(|_| ())?;
                                }
                                return Ok(());
                            }
                        }
                        Action::CantDo => {
                            match message.payload {
                                Some(Payload::CantDo(Some(CantDoReason::OutOfRangeFiatAmount | CantDoReason::OutOfRangeSatsAmount))) => {
                                    println!("Error: Amount is outside the allowed range. Please check the order's min/max limits.");
                                    return Err(());
                                }
                                Some(Payload::CantDo(Some(CantDoReason::PendingOrderExists))) => {
                                        println!("Error: A pending order already exists. Please wait for it to be filled or canceled.");
                                        return Err(());
                                    }
                                Some(Payload::CantDo(Some(CantDoReason::InvalidTradeIndex))) => {
                                    println!("Error: Invalid trade index. Please synchronize the trade index with mostro");
                                    return Err(());
                                }
                                _ => {
                                    println!("Unknown reason: {:?}", message.payload);
                                    return Err(());
                                }
                            }
                        }
                        // this is the case where the user cancels the order
                        Action::Canceled => {
                            if let Some(order_id) = &message.id {
                            // Acquire database connection
                            let pool = connect().await.map_err(|_| ())?;
                            // Verify order exists before deletion
                            if Order::get_by_id(&pool, &order_id.to_string()).await.is_ok() {
                                Order::delete_by_id(&pool, &order_id.to_string())
                                    .await
                                    .map_err(|_| ())?;
                                // Release database connection
                                drop(pool);
                                println!("Order {} canceled!", order_id);
                                return Ok(());
                            } else {
                                println!("Order not found: {}", order_id);
                                return Err(());
                                }
                            }
                        }
                        _ => {
                            println!("Unknown action: {:?}", message.action);
                            return Err(());
                        }
                    }
                    }
                }
        }
        }
        Ok(())
    })
    .await {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::anyhow!("Timeout waiting for DM or gift wrap event"))
    }
}

pub async fn send_dm(
    client: &Client,
    identity_keys: Option<&Keys>,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    payload: String,
    expiration: Option<Timestamp>,
    to_user: bool,
) -> Result<()> {
    let pow: u8 = var("POW").unwrap_or('0'.to_string()).parse().unwrap();
    let private = var("SECRET")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .unwrap();
    let event = if to_user {
        // Derive conversation key
        let ck = ConversationKey::derive(trade_keys.secret_key(), receiver_pubkey)?;
        // Encrypt payload
        let encrypted_content = encrypt_to_bytes(&ck, payload.as_bytes())?;
        // Encode with base64
        let b64decoded_content = general_purpose::STANDARD.encode(encrypted_content);
        // Compose builder
        EventBuilder::new(nostr_sdk::Kind::PrivateDirectMessage, b64decoded_content)
            .pow(pow)
            .tag(Tag::public_key(*receiver_pubkey))
            .sign_with_keys(trade_keys)?
    } else if private {
        let message = Message::from_json(&payload).unwrap();
        // We compose the content, when private we don't sign the payload
        let content: (Message, Option<Signature>) = (message, None);
        let content = serde_json::to_string(&content).unwrap();
        // We create the rumor
        let rumor = EventBuilder::text_note(content)
            .pow(pow)
            .build(trade_keys.public_key());
        let mut tags: Vec<Tag> = Vec::with_capacity(1 + usize::from(expiration.is_some()));

        if let Some(timestamp) = expiration {
            tags.push(Tag::expiration(timestamp));
        }
        let tags = Tags::from_list(tags);

        EventBuilder::gift_wrap(trade_keys, receiver_pubkey, rumor, tags).await?
    } else {
        let identity_keys = identity_keys
            .ok_or_else(|| Error::msg("identity_keys required when to_user is false"))?;
        // We sign the message
        let message = Message::from_json(&payload).unwrap();
        let sig = Message::sign(payload.clone(), trade_keys);
        // We compose the content
        let content = serde_json::to_string(&(message, sig)).unwrap();
        // We create the rumor
        let rumor = EventBuilder::text_note(content)
            .pow(pow)
            .build(trade_keys.public_key());
        let mut tags: Vec<Tag> = Vec::with_capacity(1 + usize::from(expiration.is_some()));

        if let Some(timestamp) = expiration {
            tags.push(Tag::expiration(timestamp));
        }
        let tags = Tags::from_list(tags);

        EventBuilder::gift_wrap(identity_keys, receiver_pubkey, rumor, tags).await?
    };

    client.send_event(&event).await?;

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

pub async fn send_message_sync(
    client: &Client,
    identity_keys: Option<&Keys>,
    trade_keys: &Keys,
    receiver_pubkey: PublicKey,
    message: Message,
    _wait_for_dm: bool,
    to_user: bool,
) -> Result<()> {
    let message_json = message
        .as_json()
        .map_err(|_| Error::msg("Failed to serialize message"))?;

    send_dm(
        client,
        identity_keys,
        trade_keys,
        &receiver_pubkey,
        message_json,
        None,
        to_user,
    )
    .await?;

    Ok(())
}

pub async fn get_direct_messages(
    client: &Client,
    my_key: &Keys,
    since: i64,
    from_user: bool,
) -> Vec<(Message, u64)> {
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
            .kind(nostr_sdk::Kind::PrivateDirectMessage)
            .pubkey(my_key.public_key())
            .since(timestamp)
    } else {
        Filter::new()
            .kind(nostr_sdk::Kind::GiftWrap)
            .pubkey(my_key.public_key())
            .since(fake_timestamp)
    };

    info!("Request events with event kind : {:?} ", filters.kinds);

    let mut direct_messages: Vec<(Message, u64)> = Vec::new();

    if let Ok(mostro_req) = client.fetch_events(filters, Duration::from_secs(15)).await {
        // Buffer vector for direct messages
        // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
        let mut id_list = Vec::<EventId>::new();

        for dm in mostro_req.iter() {
            if !id_list.contains(&dm.id) {
                id_list.push(dm.id);
                let (created_at, message) = if from_user {
                    let ck =
                        if let Ok(ck) = ConversationKey::derive(my_key.secret_key(), &dm.pubkey) {
                            ck
                        } else {
                            continue;
                        };
                    let b64decoded_content =
                        match general_purpose::STANDARD.decode(dm.content.as_bytes()) {
                            Ok(b64decoded_content) => b64decoded_content,
                            Err(_) => {
                                continue;
                            }
                        };

                    let unencrypted_content = decrypt_to_bytes(&ck, &b64decoded_content)
                        .expect("Failed to decrypt message");

                    let message =
                        String::from_utf8(unencrypted_content).expect("Found invalid UTF-8");
                    let message = Message::from_json(&message).expect("Failed on deserializing");

                    (dm.created_at, message)
                } else {
                    let unwrapped_gift = match nip59::extract_rumor(my_key, dm).await {
                        Ok(u) => u,
                        Err(_) => {
                            println!("Error unwrapping gift");
                            continue;
                        }
                    };
                    let (message, _): (Message, Option<String>) =
                        serde_json::from_str(&unwrapped_gift.rumor.content).unwrap();

                    (unwrapped_gift.rumor.created_at, message)
                };

                // Here we discard messages older than the real since parameter
                let since_time = chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(30))
                    .unwrap()
                    .timestamp() as u64;
                if created_at.as_u64() < since_time {
                    continue;
                }
                direct_messages.push((message, created_at.as_u64()));
            }
        }
        // Return element sorted by second tuple element ( Timestamp )
        direct_messages.sort_by(|a, b| a.1.cmp(&b.1));
    }

    direct_messages
}

fn parse_dispute_events(events: Events) -> Vec<Dispute> {
    // Extracted Disputes List
    let mut disputes_list = Vec::<Dispute>::new();

    // Scan events to extract all disputes
    for event in events.iter() {
        let dispute = dispute_from_tags(event.tags.clone());

        if dispute.is_err() {
            error!("{dispute:?}");
            continue;
        }
        let mut dispute = dispute?;

        info!("Found Dispute id : {:?}", dispute.id);

        // Get created at field from Nostr event
        dispute.created_at = event.created_at.as_u64() as i64;
        disputes_list.push(dispute);
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
    disputes_list.sort_by(|a, b| b.created_at.cmp(&a.created_at))
}


async fn parse_dm_events(events: Events, pubkey: Keys) -> Vec<(Message,u64)>{
           // Buffer vector for direct messages
        // Vector for single order id check - maybe multiple relay could send the same order id? Check unique one...
        let mut id_list = Vec::<EventId>::new();
        // Vector for direct messages
        let mut direct_messages: Vec<(Message, u64)> = Vec::new();

        for dm in events.iter() {
            if !id_list.contains(&dm.id) {
                id_list.push(dm.id);

                let unwrapped_gift = match nip59::extract_rumor(&pubkey, dm).await {
                    Ok(u) => u,
                    Err(_) => {
                        println!("Error unwrapping gift");
                        continue;
                    }
                };
                let (message, _): (Message, Option<String>) =
                    serde_json::from_str(&unwrapped_gift.rumor.content).unwrap();

                // Create a tuple with the created_at and the message
                let (created_at, message) = (unwrapped_gift.rumor.created_at, message);
                

                // Here we discard messages older than the real since parameter
                let since_time = chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(30))
                    .unwrap()
                    .timestamp() as u64;
                if created_at.as_u64() < since_time {
                    continue;
                }
                direct_messages.push((message, created_at.as_u64()));
            }
        }
        // Return element sorted by second tuple element ( Timestamp )
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));

    direct_messages
}

fn parse_orders_events(
    events: Events,
    currency: Option<String>,
    status: Option<Status>,
    kind: Option<mostro_core::order::Kind>,
) -> Vec<SmallOrder> {
    // Extracted Orders List
    let mut complete_events_list = Vec::<SmallOrder>::new();
    let mut requested_orders_list = Vec::<SmallOrder>::new();

    // Scan events to extract all orders
    for event in events.iter() {
        let order = order_from_tags(event.tags.clone());

        if order.is_err() {
            error!("{order:?}");
            continue;
        }
        if let Ok(mut order) = order {

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
            order.created_at = Some(event.created_at.as_u64() as i64);
            complete_events_list.push(order.clone());
            if order.status.ne(&status) {
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
    }
    // Finally sort list by creation time
    requested_orders_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    requested_orders_list
}

fn create_filter(list_kind: ListKind, pubkey: PublicKey) -> Filter {
    match list_kind {
        ListKind::Orders => {
            let since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::days(7))
                .unwrap()
                .timestamp() as u64;

            let timestamp = Timestamp::from(since_time);

            // Create filter for fetching orders
            Filter::new()
                .author(pubkey)
                .limit(50)
                .since(timestamp)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), "order".to_string())
                .kind(nostr_sdk::Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND))
        }
        ListKind::Disputes => {
            let since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::days(7))
                .unwrap()
                .timestamp() as u64;

            let timestamp = Timestamp::from(since_time);

            // Create filter for fetching orders
            Filter::new()
                .author(pubkey)
                .limit(50)
                .since(timestamp)
                .custom_tag(
                    SingleLetterTag::lowercase(Alphabet::Z),
                    "dispute".to_string(),
                )
                .kind(nostr_sdk::Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND))
        }
        ListKind::DirectMessages => {
            // We use a fake timestamp to thwart time-analysis attacks
            let fake_since = 2880;
            let fake_since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::minutes(fake_since))
                .unwrap()
                .timestamp() as u64;

            let fake_timestamp = Timestamp::from(fake_since_time);

            Filter::new()
                .kind(nostr_sdk::Kind::GiftWrap)
                .pubkey(pubkey)
                .since(fake_timestamp)
        }
    }
}


pub async fn fetch_events_list<T>(
    pubkey: PublicKey,
    list_kind: ListKind,
    status: Option<Status>,
    currency: Option<String>,
    kind: Option<mostro_core::order::Kind>,
    client: &Client,
) -> Result<Vec<Event>> {
    // Create filter for fetching orders
    let filters = create_filter(ListKind::Orders, pubkey);

    // Send all requests to relays
    if let Ok(fetched_events) = client.fetch_events(filters, Duration::from_secs(15)).await {
        match list_kind {
            ListKind::Orders => {
                info!("Fetching orders for pubkey: {}", pubkey);
                Ok(parse_orders_events(fetched_events, currency, status, kind)
                    .into_iter()
                    .map(Event::SmallOrder)
                    .collect()
                )
            }
            ListKind::DirectMessages => {
                info!("Fetching direct messages for pubkey: {}", pubkey);
                 Ok(parse_dm_events(fetched_events, pubkey).await
                    .into_iter()
                    .map(Event::MessageTuple)
                    .collect()
                )
            }
            ListKind::Disputes => {
                info!("Fetching disputes for pubkey: {}", pubkey);
                Ok(parse_dispute_events(fetched_events)
                    .into_iter()
                    .map(Event::Dispute)
                    .collect()
                )
            }
        }
    }
    else {
        Err(anyhow::anyhow!("Error in fetching events request"))
    }
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
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::Z),
            "dispute".to_string(),
        )
        .kind(nostr_sdk::Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND));

    // Extracted Orders List
    let mut disputes_list = Vec::<Dispute>::new();

    // Send all requests to relays
    if let Ok(mostro_req) = client.fetch_events(filter, Duration::from_secs(15)).await {
        // Scan events to extract all disputes
        for d in mostro_req.iter() {
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
