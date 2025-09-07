use crate::cli::send_msg::execute_send_msg;
use crate::cli::Commands;
use crate::db::{connect, Order, User};
use crate::parser::{parse_dispute_events, parse_dm_events, parse_orders_events};
use anyhow::{Error, Result};
use base64::engine::general_purpose;
use base64::Engine;
use dotenvy::var;
use mostro_core::prelude::*;
use nip44::v2::{encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::time::Duration;
use std::{fs, path::Path};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum Event {
    SmallOrder(SmallOrder),
    Dispute(Dispute), // Assuming you have a Dispute struct
    MessageTuple(Box<(Message, u64)>),
}

#[derive(Clone, Debug)]
pub enum ListKind {
    Orders,
    Disputes,
    DirectMessagesUser,
    DirectMessagesAdmin,
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

pub fn create_filter(list_kind: ListKind, pubkey: PublicKey) -> Filter {
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
        ListKind::DirectMessagesAdmin => {
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
        ListKind::DirectMessagesUser => {
            let since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::minutes(30))
                .unwrap()
                .timestamp() as u64;
            let timestamp = Timestamp::from(since_time);
            Filter::new()
                .kind(nostr_sdk::Kind::PrivateDirectMessage)
                .pubkey(pubkey)
                .since(timestamp)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_events_list(
    list_kind: ListKind,
    status: Option<Status>,
    currency: Option<String>,
    kind: Option<mostro_core::order::Kind>,
    mostro_pubkey: PublicKey,
    mostro_keys: &Keys,
    trade_index: i64,
    pool: &SqlitePool,
    client: &Client,
) -> Result<Vec<Event>> {
    match list_kind {
        ListKind::Orders => {
            let filters = create_filter(list_kind, mostro_pubkey);
            let fetched_events = client
                .fetch_events(filters, Duration::from_secs(15))
                .await?;
            let orders = parse_orders_events(fetched_events, currency, status, kind);
            Ok(orders.into_iter().map(Event::SmallOrder).collect())
        }
        ListKind::DirectMessagesAdmin => {
            let filters = create_filter(list_kind, mostro_keys.public_key());
            let fetched_events = client
                .fetch_events(filters, Duration::from_secs(15))
                .await?;
            let direct_messages_mostro = parse_dm_events(fetched_events, mostro_keys).await;
            Ok(direct_messages_mostro
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::DirectMessagesUser => {
            let mut direct_messages: Vec<(Message, u64)> = Vec::new();
            for index in 1..=trade_index {
                let trade_key = User::get_trade_keys(pool, index).await?;
                let filter = create_filter(ListKind::DirectMessagesUser, trade_key.public_key());
                let fetched_user_messages =
                    client.fetch_events(filter, Duration::from_secs(15)).await?;
                let direct_messages_for_trade_key =
                    parse_dm_events(fetched_user_messages, &trade_key).await;
                direct_messages.extend(direct_messages_for_trade_key);
            }
            Ok(direct_messages
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::Disputes => {
            let filters = create_filter(list_kind, mostro_pubkey);
            let fetched_events = client
                .fetch_events(filters, Duration::from_secs(15))
                .await?;
            let disputes = parse_dispute_events(fetched_events);
            Ok(disputes.into_iter().map(Event::Dispute).collect())
        }
    }
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

pub async fn run_simple_order_msg(
    command: Commands,
    order_id: &Uuid,
    identity_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    execute_send_msg(
        command,
        Some(*order_id),
        Some(identity_keys),
        mostro_key,
        client,
        None,
    )
    .await
}
