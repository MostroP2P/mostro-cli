use crate::cli::send_msg::execute_send_msg;
use crate::cli::{Commands, Context};
use crate::db::{Order, User};
use crate::parser::dms::print_commands_results;
use crate::parser::{parse_dispute_events, parse_dm_events, parse_orders_events};
use anyhow::{Error, Result};
use base64::engine::general_purpose;
use base64::Engine;
use dotenvy::var;
use log::info;
use mostro_core::prelude::*;
use nip44::v2::{encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::future::Future;
use std::time::Duration;
use std::{fs, path::Path};
use uuid::Uuid;

const FAKE_SINCE: i64 = 2880;
const FETCH_EVENTS_TIMEOUT: Duration = Duration::from_secs(15);

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
    PrivateDirectMessagesUser,
}

async fn send_gift_wrap_dm_internal(
    client: &Client,
    sender_keys: &Keys,
    receiver_pubkey: &PublicKey,
    message: &str,
    is_admin: bool,
) -> Result<()> {
    let pow: u8 = var("POW")
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0);

    // Create Message struct for consistency with Mostro protocol
    let dm_message = Message::new_dm(
        None,
        None,
        Action::SendDm,
        Some(Payload::TextMessage(message.to_string())),
    );

    // Serialize as JSON with the expected format (Message, Option<Signature>)
    let content = serde_json::to_string(&(dm_message, None::<String>))?;

    // Create the rumor with JSON content
    let rumor = EventBuilder::text_note(content)
        .pow(pow)
        .build(sender_keys.public_key());

    // Create gift wrap using sender_keys as the signing key
    let event = EventBuilder::gift_wrap(sender_keys, receiver_pubkey, rumor, Tags::new()).await?;

    let sender_type = if is_admin { "admin" } else { "user" };
    info!(
        "Sending {} gift wrap event to {}",
        sender_type, receiver_pubkey
    );
    client.send_event(&event).await?;

    Ok(())
}

pub async fn send_admin_gift_wrap_dm(
    client: &Client,
    admin_keys: &Keys,
    receiver_pubkey: &PublicKey,
    message: &str,
) -> Result<()> {
    send_gift_wrap_dm_internal(client, admin_keys, receiver_pubkey, message, true).await
}

pub async fn send_gift_wrap_dm(
    client: &Client,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    message: &str,
) -> Result<()> {
    send_gift_wrap_dm_internal(client, trade_keys, receiver_pubkey, message, false).await
}

pub async fn save_order(
    order: SmallOrder,
    trade_keys: &Keys,
    request_id: u64,
    trade_index: i64,
    pool: &SqlitePool,
) -> Result<()> {
    if let Ok(order) = Order::new(pool, order, trade_keys, Some(request_id as i64)).await {
        if let Some(order_id) = order.id {
            println!("Order {} created", order_id);
        } else {
            println!("Warning: The newly created order has no ID.");
        }

        // Update last trade index to be used in next trade
        match User::get(pool).await {
            Ok(mut user) => {
                user.set_last_trade_index(trade_index);
                if let Err(e) = user.save(pool).await {
                    println!("Failed to update user: {}", e);
                }
            }
            Err(e) => println!("Failed to get user: {}", e),
        }
    }
    Ok(())
}

/// Wait for incoming gift wraps or events coming in
pub async fn wait_for_dm<F>(
    ctx: &Context,
    order_trade_keys: Option<&Keys>,
    sent_message: F,
) -> anyhow::Result<Events>
where
    F: Future<Output = Result<()>> + Send,
{
    // Get correct trade keys to wait for
    let trade_keys = order_trade_keys.unwrap_or(&ctx.trade_keys);
    // Get notifications from client
    let mut notifications = ctx.client.notifications();
    // Create subscription
    let opts =
        SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEventsAfterEOSE(1));
    // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
    let subscription = Filter::new()
        .pubkey(trade_keys.public_key())
        .kind(nostr_sdk::Kind::GiftWrap)
        .limit(0);
    // Subscribe to subscription with exit policy of just waiting for 1 event
    ctx.client.subscribe(subscription, Some(opts)).await?;

    // Await the sent message
    sent_message.await?;

    // Wait for event
    let event = tokio::time::timeout(FETCH_EVENTS_TIMEOUT, async move {
        loop {
            match notifications.recv().await {
                Ok(notification) => {
                    match notification {
                        RelayPoolNotification::Event { event, .. } => {
                            // Return event
                            return Ok(*event);
                        }
                        _ => {
                            // Continue waiting for a valid event
                            continue;
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error receiving notification: {:?}", e));
                }
            }
        }
    })
    .await?
    .map_err(|_| anyhow::anyhow!("Timeout waiting for DM or gift wrap event"))?;

    // Convert event to events
    let mut events = Events::default();
    events.insert(event);
    Ok(events)
}

#[derive(Debug, Clone, Copy)]
enum MessageType {
    PrivateDirectMessage,
    PrivateGiftWrap,
    SignedGiftWrap,
}

fn determine_message_type(to_user: bool, private: bool) -> MessageType {
    match (to_user, private) {
        (true, _) => MessageType::PrivateDirectMessage,
        (false, true) => MessageType::PrivateGiftWrap,
        (false, false) => MessageType::SignedGiftWrap,
    }
}

fn create_expiration_tags(expiration: Option<Timestamp>) -> Tags {
    let mut tags: Vec<Tag> = Vec::with_capacity(1 + usize::from(expiration.is_some()));

    if let Some(timestamp) = expiration {
        tags.push(Tag::expiration(timestamp));
    }

    Tags::from_list(tags)
}

async fn create_private_dm_event(
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    payload: String,
    pow: u8,
) -> Result<nostr_sdk::Event> {
    // Derive conversation key
    let ck = ConversationKey::derive(trade_keys.secret_key(), receiver_pubkey)?;
    // Encrypt payload
    let encrypted_content = encrypt_to_bytes(&ck, payload.as_bytes())?;
    // Encode with base64
    let b64decoded_content = general_purpose::STANDARD.encode(encrypted_content);
    // Compose builder
    Ok(
        EventBuilder::new(nostr_sdk::Kind::PrivateDirectMessage, b64decoded_content)
            .pow(pow)
            .tag(Tag::public_key(*receiver_pubkey))
            .sign_with_keys(trade_keys)?,
    )
}

async fn create_gift_wrap_event(
    trade_keys: &Keys,
    identity_keys: Option<&Keys>,
    receiver_pubkey: &PublicKey,
    payload: String,
    pow: u8,
    expiration: Option<Timestamp>,
    signed: bool,
) -> Result<nostr_sdk::Event> {
    let message = Message::from_json(&payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize message: {e}"))?;

    let content = if signed {
        let _identity_keys = identity_keys
            .ok_or_else(|| Error::msg("identity_keys required for signed messages"))?;
        // We sign the message
        let sig = Message::sign(payload, trade_keys);
        serde_json::to_string(&(message, sig))
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    } else {
        // We compose the content, when private we don't sign the payload
        let content: (Message, Option<Signature>) = (message, None);
        serde_json::to_string(&content)
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    };

    // We create the rumor
    let rumor = EventBuilder::text_note(content)
        .pow(pow)
        .build(trade_keys.public_key());

    let tags = create_expiration_tags(expiration);

    let signer_keys = if signed {
        identity_keys.ok_or_else(|| Error::msg("identity_keys required for signed messages"))?
    } else {
        trade_keys
    };

    Ok(EventBuilder::gift_wrap(signer_keys, receiver_pubkey, rumor, tags).await?)
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
    let pow: u8 = var("POW")
        .unwrap_or('0'.to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse POW: {}", e))?;
    let private = var("SECRET")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .map_err(|e| anyhow::anyhow!("Failed to parse SECRET: {}", e))?;

    let message_type = determine_message_type(to_user, private);

    let event = match message_type {
        MessageType::PrivateDirectMessage => {
            create_private_dm_event(trade_keys, receiver_pubkey, payload, pow).await?
        }
        MessageType::PrivateGiftWrap => {
            create_gift_wrap_event(
                trade_keys,
                identity_keys,
                receiver_pubkey,
                payload,
                pow,
                expiration,
                false,
            )
            .await?
        }
        MessageType::SignedGiftWrap => {
            create_gift_wrap_event(
                trade_keys,
                identity_keys,
                receiver_pubkey,
                payload,
                pow,
                expiration,
                true,
            )
            .await?
        }
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

pub async fn get_direct_messages_from_trade_keys(
    client: &Client,
    trade_keys_hex: Vec<String>,
    since: i64,
    _mostro_pubkey: &PublicKey,
) -> Result<Vec<(Message, u64, PublicKey)>> {
    if trade_keys_hex.is_empty() {
        return Ok(Vec::new());
    }

    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(since))
        .ok_or(anyhow::anyhow!("Failed to get since time"))?
        .timestamp();

    // Get the triple of message, timestamp and public key
    let mut all_messages: Vec<(Message, u64, PublicKey)> = Vec::new();

    // Fetch direct messages from trade keys and in case of since, we filter by since
    // as bonus we also fetch the events from the admin pubkey in case is specified
    for trade_key_hex in trade_keys_hex {
        if let Ok(public_key) = PublicKey::from_hex(&trade_key_hex) {
            // Create filter for fetching direct messages
            let filter =
                create_filter(ListKind::DirectMessagesUser, public_key, Some(&since_time))?;
            let events = client.fetch_events(filter, FETCH_EVENTS_TIMEOUT).await?;
            // Parse events without keys since we only have the public key
            // We'll need to handle this differently - let's just collect the events for now
            for event in events {
                if let Ok(message) = Message::from_json(&event.content) {
                    if event.created_at.as_u64() < since as u64 {
                        continue;
                    }
                    all_messages.push((message, event.created_at.as_u64(), event.pubkey));
                }
            }
        }
    }
    Ok(all_messages)
}

/// Create a fake timestamp to thwart time-analysis attacks
fn create_fake_timestamp() -> Result<Timestamp> {
    let fake_since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(FAKE_SINCE))
        .ok_or(anyhow::anyhow!("Failed to get fake since time"))?
        .timestamp() as u64;
    Ok(Timestamp::from(fake_since_time))
}

// Create a filter for fetching events in the last 7 days
fn create_seven_days_filter(letter: Alphabet, value: String, pubkey: PublicKey) -> Result<Filter> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .ok_or(anyhow::anyhow!("Failed to get since days ago"))?
        .timestamp() as u64;

    let timestamp = Timestamp::from(since_time);

    Ok(Filter::new()
        .author(pubkey)
        .limit(50)
        .since(timestamp)
        .custom_tag(SingleLetterTag::lowercase(letter), value)
        .kind(nostr_sdk::Kind::Custom(NOSTR_REPLACEABLE_EVENT_KIND)))
}

// Create a filter for fetching events
pub fn create_filter(
    list_kind: ListKind,
    pubkey: PublicKey,
    since: Option<&i64>,
) -> Result<Filter> {
    match list_kind {
        ListKind::Orders => create_seven_days_filter(Alphabet::Z, "order".to_string(), pubkey),
        ListKind::Disputes => create_seven_days_filter(Alphabet::Z, "dispute".to_string(), pubkey),
        ListKind::DirectMessagesAdmin | ListKind::DirectMessagesUser => {
            // We use a fake timestamp to thwart time-analysis attacks
            let fake_timestamp = create_fake_timestamp()?;

            Ok(Filter::new()
                .kind(nostr_sdk::Kind::GiftWrap)
                .pubkey(pubkey)
                .since(fake_timestamp))
        }
        ListKind::PrivateDirectMessagesUser => {
            // Get since from cli or use 30 minutes default
            let since = if let Some(mins) = since {
                chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(*mins))
                    .unwrap()
                    .timestamp()
            } else {
                chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(30))
                    .unwrap()
                    .timestamp()
            } as u64;
            // Create filter for fetching privatedirect messages
            Ok(Filter::new()
                .kind(nostr_sdk::Kind::PrivateDirectMessage)
                .pubkey(pubkey)
                .since(Timestamp::from(since)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_events_list(
    list_kind: ListKind,
    status: Option<Status>,
    currency: Option<String>,
    kind: Option<mostro_core::order::Kind>,
    ctx: &Context,
    since: Option<&i64>,
) -> Result<Vec<Event>> {
    match list_kind {
        ListKind::Orders => {
            let filters = create_filter(list_kind, ctx.mostro_pubkey, None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
                .await?;
            let orders = parse_orders_events(fetched_events, currency, status, kind);
            Ok(orders.into_iter().map(Event::SmallOrder).collect())
        }
        ListKind::DirectMessagesAdmin => {
            // Fetch gift wraps sent TO the admin's public key (not Mostro's)
            let filters = create_filter(list_kind, ctx.context_keys.public_key(), None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
                .await?;
            let direct_messages_mostro =
                parse_dm_events(fetched_events, &ctx.context_keys, since).await;
            Ok(direct_messages_mostro
                .into_iter()
                .map(|(message, timestamp, _)| Event::MessageTuple(Box::new((message, timestamp))))
                .collect())
        }
        ListKind::PrivateDirectMessagesUser => {
            let mut direct_messages: Vec<(Message, u64)> = Vec::new();
            for index in 1..=ctx.trade_index {
                let trade_key = User::get_trade_keys(&ctx.pool, index).await?;
                let filter = create_filter(
                    ListKind::PrivateDirectMessagesUser,
                    trade_key.public_key(),
                    None,
                )?;
                let fetched_user_messages = ctx
                    .client
                    .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
                    .await?;
                let direct_messages_for_trade_key =
                    parse_dm_events(fetched_user_messages, &trade_key, since).await;
                direct_messages.extend(
                    direct_messages_for_trade_key
                        .into_iter()
                        .map(|(message, timestamp, _)| (message, timestamp)),
                );
            }
            Ok(direct_messages
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::DirectMessagesUser => {
            let mut direct_messages: Vec<(Message, u64)> = Vec::new();

            for index in 1..=ctx.trade_index {
                let trade_key = User::get_trade_keys(&ctx.pool, index).await?;
                let filter =
                    create_filter(ListKind::DirectMessagesUser, trade_key.public_key(), None)?;
                let fetched_user_messages = ctx
                    .client
                    .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
                    .await?;
                let direct_messages_for_trade_key =
                    parse_dm_events(fetched_user_messages, &trade_key, since).await;
                direct_messages.extend(
                    direct_messages_for_trade_key
                        .into_iter()
                        .map(|(message, timestamp, _)| (message, timestamp)),
                );
            }
            Ok(direct_messages
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::Disputes => {
            let filters = create_filter(list_kind, ctx.mostro_pubkey, None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
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
    order_id: Option<Uuid>,
    ctx: &Context,
) -> Result<()> {
    execute_send_msg(command, order_id, ctx, None).await
}

// helper (place near other CLI utils)
pub async fn admin_send_dm(ctx: &Context, msg: String) -> anyhow::Result<()> {
    send_dm(
        &ctx.client,
        Some(&ctx.context_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        msg,
        None,
        false,
    )
    .await?;
    Ok(())
}

pub async fn print_dm_events(
    recv_event: Events,
    request_id: u64,
    ctx: &Context,
    order_trade_keys: Option<&Keys>,
) -> Result<()> {
    // Get the trade keys
    let trade_keys = order_trade_keys.unwrap_or(&ctx.trade_keys);
    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, trade_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message = message.get_inner_message_kind();
        if message.request_id == Some(request_id) {
            print_commands_results(message, ctx).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Received response with mismatched request_id. Expected: {}, Got: {:?}",
                request_id,
                message.request_id
            ));
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {}
