use anyhow::{Error, Result};
use base64::engine::general_purpose;
use base64::Engine;
use log::info;
use mostro_core::prelude::*;
use nip44::v2::{encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use std::env::var;

use crate::cli::Context;
use crate::parser::dms::print_commands_results;
use crate::parser::parse_dm_events;
use crate::util::types::MessageType;

/// Helper function to retrieve and validate admin keys from context
pub fn get_admin_keys(ctx: &Context) -> Result<&Keys> {
    let admin_keys = ctx.context_keys.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Admin keys not available. ADMIN_NSEC must be set for admin commands.")
    })?;

    // Only log admin public key in verbose mode
    if std::env::var("RUST_LOG").is_ok() {
        println!("🔑 Admin Keys: {}", admin_keys.public_key);
    }

    Ok(admin_keys)
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

    let dm_message = Message::new_dm(
        None,
        None,
        Action::SendDm,
        Some(Payload::TextMessage(message.to_string())),
    );

    let content = serde_json::to_string(&(dm_message, None::<String>))?;

    let rumor = EventBuilder::text_note(content).build(sender_keys.public_key());
    let seal: Event = EventBuilder::seal(sender_keys, receiver_pubkey, rumor)
        .await?
        .sign(sender_keys)
        .await?;
    let event = gift_wrap_from_seal_with_pow(receiver_pubkey, &seal, Tags::new(), pow)?;

    let sender_type = if is_admin { "admin" } else { "user" };
    info!(
        "Sending {} gift wrap event to {}",
        sender_type, receiver_pubkey
    );
    client.send_event(&event).await?;

    Ok(())
}

pub async fn wait_for_dm<F>(
    ctx: &crate::cli::Context,
    order_trade_keys: Option<&Keys>,
    sent_message: F,
) -> anyhow::Result<Events>
where
    F: std::future::Future<Output = Result<()>> + Send,
{
    let trade_keys = order_trade_keys.unwrap_or(&ctx.trade_keys);
    let mut notifications = ctx.client.notifications();
    let opts =
        SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEventsAfterEOSE(1));
    let subscription = Filter::new()
        .pubkey(trade_keys.public_key())
        .kind(nostr_sdk::Kind::GiftWrap)
        .limit(0);
    ctx.client.subscribe(subscription, Some(opts)).await?;

    // Send message here after opening notifications to avoid missing messages.
    sent_message.await?;

    // Wait for the DM or gift wrap event
    let event = tokio::time::timeout(super::events::FETCH_EVENTS_TIMEOUT, async move {
        loop {
            match notifications.recv().await {
                Ok(notification) => match notification {
                    RelayPoolNotification::Event { event, .. } => {
                        return Ok(*event);
                    }
                    _ => continue,
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("Error receiving notification: {:?}", e));
                }
            }
        }
    })
    .await?
    .map_err(|_| anyhow::anyhow!("Timeout waiting for DM or gift wrap event"))?;

    let mut events = Events::default();
    events.insert(event);
    Ok(events)
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
    let ck = ConversationKey::derive(trade_keys.secret_key(), receiver_pubkey)?;
    let encrypted_content = encrypt_to_bytes(&ck, payload.as_bytes())?;
    let b64decoded_content = general_purpose::STANDARD.encode(encrypted_content);
    Ok(
        EventBuilder::new(nostr_sdk::Kind::PrivateDirectMessage, b64decoded_content)
            .pow(pow)
            .tag(Tag::public_key(*receiver_pubkey))
            .sign_with_keys(trade_keys)?,
    )
}

/// Builds the published NIP-59 **Gift Wrap** (kind 1059) from a signed **Seal** event.
///
/// Rust-nostr’s `EventBuilder::gift_wrap` seals and wraps but does not apply NIP-13 PoW to the
/// outer Gift Wrap; Mostro may require that difficulty on the relay-visible event. This helper
/// mirrors the SDK’s seal→wrap steps: reject non-seal inputs, encrypt the seal JSON to `receiver`
/// with NIP-44 using an **ephemeral** key pair, attach `p` and optional tags, set
/// [`nip59::RANGE_RANDOM_TIMESTAMP_TWEAK`]-style `created_at`, mine with [`EventBuilder::pow`],
/// then sign the wrap with the ephemeral keys.
fn gift_wrap_from_seal_with_pow(
    receiver: &PublicKey,
    seal: &Event,
    extra_tags: impl IntoIterator<Item = Tag>,
    pow: u8,
) -> Result<Event> {
    if seal.kind != nostr_sdk::Kind::Seal {
        return Err(anyhow::anyhow!("Invalid kind"));
    }

    let ephem = Keys::generate();
    let content = nip44::encrypt(
        ephem.secret_key(),
        receiver,
        seal.as_json(),
        nip44::Version::default(),
    )?;

    let mut tags: Vec<Tag> = extra_tags.into_iter().collect();
    tags.push(Tag::public_key(*receiver));

    EventBuilder::new(nostr_sdk::Kind::GiftWrap, content)
        .tags(tags)
        .custom_created_at(Timestamp::tweaked(nip59::RANGE_RANDOM_TIMESTAMP_TWEAK))
        .pow(pow)
        .sign_with_keys(&ephem)
        .map_err(|e| anyhow::anyhow!("Failed to sign gift wrap: {e}"))
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
        let sig = Message::sign(payload, trade_keys);
        serde_json::to_string(&(message, sig))
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    } else {
        let content: (Message, Option<Signature>) = (message, None);
        serde_json::to_string(&content)
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    };

    let rumor = EventBuilder::text_note(content).build(trade_keys.public_key());

    let tags = create_expiration_tags(expiration);

    let signer_keys = if signed {
        identity_keys.ok_or_else(|| Error::msg("identity_keys required for signed messages"))?
    } else {
        trade_keys
    };

    let seal: Event = EventBuilder::seal(signer_keys, receiver_pubkey, rumor)
        .await?
        .sign(signer_keys)
        .await?;

    gift_wrap_from_seal_with_pow(receiver_pubkey, &seal, tags, pow)
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

pub async fn print_dm_events(
    recv_event: Events,
    request_id: u64,
    ctx: &crate::cli::Context,
    order_trade_keys: Option<&Keys>,
) -> Result<()> {
    let trade_keys = order_trade_keys.unwrap_or(&ctx.trade_keys);
    let messages = parse_dm_events(recv_event, trade_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message = message.get_inner_message_kind();
        match message.request_id {
            Some(id) => {
                if request_id == id {
                    print_commands_results(message, ctx).await?;
                }
            }
            None if message.action == Action::RateReceived
                || message.action == Action::NewOrder =>
            {
                print_commands_results(message, ctx).await?;
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Received response with mismatched request_id. Expected: {}, Got: Null",
                    request_id,
                ));
            }
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }
    Ok(())
}
