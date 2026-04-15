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
use crate::util::events::FETCH_EVENTS_TIMEOUT;
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

/// Derive shared ECDH keys from a local keypair and a counterparty public key.
pub fn derive_shared_keys(
    admin_keys: Option<&Keys>,
    counterparty_pubkey: Option<&PublicKey>,
) -> Option<Keys> {
    let admin = admin_keys?;
    let cp_pk = counterparty_pubkey?;
    let shared_bytes = nostr_sdk::util::generate_shared_key(admin.secret_key(), cp_pk).ok()?;
    let sk = nostr_sdk::SecretKey::from_slice(&shared_bytes).ok()?;
    Some(Keys::new(sk))
}

/// Convenience wrapper: derive a shared key and return its secret as a hex string.
pub fn derive_shared_key_hex(
    admin_keys: Option<&Keys>,
    counterparty_pubkey_str: Option<&str>,
) -> Option<String> {
    let cp_pk = counterparty_pubkey_str.and_then(|s| PublicKey::parse(s).ok());
    let keys = derive_shared_keys(admin_keys, cp_pk.as_ref())?;
    Some(keys.secret_key().to_secret_hex())
}

/// Rebuild a `Keys` from a stored shared-key hex string.
pub fn keys_from_shared_hex(hex: &str) -> Option<Keys> {
    nostr_sdk::Keys::parse(hex).ok()
}

/// Derive shared secret bytes (ECDH) using the same algorithm as send_admin_dm_attach.
/// Used so the receive path can decrypt DMs sent via that flow. Returns 32 bytes suitable
/// for ChaCha20-Poly1305 or for building Keys via Keys::new(SecretKey::from_slice(&bytes)).
pub fn derive_shared_key_bytes(local_keys: &Keys, other_pubkey: &PublicKey) -> Result<[u8; 32]> {
    use bitcoin::secp256k1::ecdh::shared_secret_point;
    use bitcoin::secp256k1::{Parity, PublicKey as SecpPublicKey};

    let sk = local_keys.secret_key();
    let xonly = other_pubkey
        .xonly()
        .map_err(|_| anyhow::anyhow!("failed to get x-only public key"))?;
    let secp_pk = SecpPublicKey::from_x_only_public_key(xonly, Parity::Even);
    let mut point_bytes = shared_secret_point(&secp_pk, sk).as_slice().to_vec();
    point_bytes.resize(32, 0);
    point_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("shared secret point must be at least 32 bytes"))
}

/// Build a NIP-59 gift wrap event to a recipient pubkey (e.g. shared key pubkey).
/// Rumor content is Mostro protocol format: JSON of (Message, Option<String>).
async fn build_custom_wrap_event(
    sender_keys: &Keys,
    recipient_pubkey: &PublicKey,
    message: &str,
) -> Result<Event> {
    let inner_message = EventBuilder::text_note(message)
        .build(sender_keys.public_key())
        .sign(sender_keys)
        .await?;

    // Ephemeral key for the custom wrap
    let ephem_key = Keys::generate();

    // Encrypt the inner message with the ephemeral key using NIP-44
    let encrypted_content = nip44::encrypt(
        ephem_key.secret_key(),
        recipient_pubkey,
        inner_message.as_json(),
        nip44::Version::V2,
    )?;

    // Build tags for the wrapper event, the recipient pubkey is the shared key pubkey
    let tag = Tag::public_key(*recipient_pubkey);

    // Reuse POW behaviour from existing DM helpers, but fail on invalid values
    let pow: u8 = var("POW")
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse POW: {}", e))?;

    // Build the wrapped event
    let wrapped_event = EventBuilder::new(nostr_sdk::Kind::GiftWrap, encrypted_content)
        .tag(tag)
        .custom_created_at(Timestamp::tweaked(nip59::RANGE_RANDOM_TIMESTAMP_TWEAK))
        .pow(pow)
        .sign_with_keys(&ephem_key)?;

    Ok(wrapped_event)
}

/// Send a chat message via a per-dispute shared key (ECDH-derived).
/// The gift wrap is addressed to the shared key's public key so both parties
/// (who derive the same shared key) can fetch and decrypt the event.
pub async fn send_admin_chat_message_via_shared_key(
    client: &Client,
    sender_keys: &Keys,
    shared_keys: &Keys,
    content: &str,
) -> Result<()> {
    let content = content.trim();
    if content.is_empty() {
        return Err(anyhow::anyhow!("Cannot send empty chat message"));
    }
    let recipient_pubkey = shared_keys.public_key();
    let event = build_custom_wrap_event(sender_keys, &recipient_pubkey, content).await?;
    client.send_event(&event).await?;
    Ok(())
}

/// Unwrap a custom Mostro P2P giftwrap addressed to a shared key.
/// Decrypts with the shared key using NIP-44 and returns (content, timestamp, sender_pubkey).
pub async fn unwrap_giftwrap_with_shared_key(
    shared_keys: &Keys,
    event: &Event,
) -> Result<(String, i64, PublicKey)> {
    let decrypted = nip44::decrypt(shared_keys.secret_key(), &event.pubkey, &event.content)
        .map_err(|e| anyhow::anyhow!("Failed to decrypt gift wrap with shared key: {e}"))?;

    let inner_event = Event::from_json(&decrypted)
        .map_err(|e| anyhow::anyhow!("Invalid inner chat event: {e}"))?;

    inner_event
        .verify()
        .map_err(|e| anyhow::anyhow!("Invalid inner chat event signature: {e}"))?;

    Ok((
        inner_event.content,
        inner_event.created_at.as_u64() as i64,
        inner_event.pubkey,
    ))
}

/// Fetch gift wrap events addressed to a specific shared key's public key,
/// decrypt each with the shared key, and return (content, timestamp, sender_pubkey).
pub async fn fetch_gift_wraps_for_shared_key(
    client: &Client,
    shared_keys: &Keys,
) -> Result<Vec<(String, i64, PublicKey)>> {
    let now = Timestamp::now().as_u64();
    let seven_days_secs: u64 = 7 * 24 * 60 * 60;
    let wide_since = now.saturating_sub(seven_days_secs);

    let shared_pubkey = shared_keys.public_key();
    let filter = Filter::new()
        .kind(nostr_sdk::Kind::GiftWrap)
        .pubkey(shared_pubkey)
        .since(Timestamp::from(wide_since))
        .limit(100);

    let events = client
        .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch chat events for shared key: {e}"))?;

    let mut messages = Vec::new();
    for wrapped in events.iter() {
        let to_shared = wrapped.tags.public_keys().any(|pk| *pk == shared_pubkey);
        if !to_shared {
            continue;
        }
        match unwrap_giftwrap_with_shared_key(shared_keys, wrapped).await {
            Ok((content, ts, sender_pubkey)) => {
                messages.push((content, ts, sender_pubkey));
            }
            Err(e) => {
                log::warn!(
                    "Failed to unwrap gift wrap for shared key {}: {}",
                    wrapped.id,
                    e
                );
            }
        }
    }
    messages.sort_by_key(|(_, ts, _)| *ts);
    Ok(messages)
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
        .map_err(|e| anyhow::anyhow!("Failed to parse POW: {}", e))?;

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
        return Err(anyhow::anyhow!(
            "Expected Seal (kind {}), got kind {}",
            nostr_sdk::Kind::Seal.as_u16(),
            seal.kind.as_u16(),
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn leading_zero_bits_in_hex(hex: &str) -> u32 {
        let mut bits = 0_u32;
        for ch in hex.chars() {
            let nibble = ch.to_digit(16).expect("event id must be hex");
            if nibble == 0 {
                bits += 4;
            } else {
                bits += nibble.leading_zeros() - 28;
                break;
            }
        }
        bits
    }

    fn event_meets_pow(event: &Event, difficulty: u8) -> bool {
        let id_hex = event.id.to_string();
        leading_zero_bits_in_hex(&id_hex) >= difficulty.into()
    }

    #[test]
    fn gift_wrap_from_seal_with_pow_builds_gift_wrap_kind() -> Result<()> {
        let receiver = Keys::generate().public_key();
        let seal = EventBuilder::new(nostr_sdk::Kind::Seal, "sealed payload")
            .sign_with_keys(&Keys::generate())?;

        let event = gift_wrap_from_seal_with_pow(&receiver, &seal, Tags::new(), 0)?;

        assert_eq!(event.kind, nostr_sdk::Kind::GiftWrap);
        Ok(())
    }

    #[test]
    fn gift_wrap_from_seal_with_pow_meets_requested_difficulty() -> Result<()> {
        let receiver = Keys::generate().public_key();
        let seal = EventBuilder::new(nostr_sdk::Kind::Seal, "sealed payload")
            .sign_with_keys(&Keys::generate())?;
        let pow = 8;

        let event = gift_wrap_from_seal_with_pow(&receiver, &seal, Tags::new(), pow)?;

        assert!(
            event_meets_pow(&event, pow),
            "gift wrap id does not satisfy PoW"
        );
        Ok(())
    }

    #[test]
    fn gift_wrap_from_seal_with_pow_rejects_non_seal() {
        let receiver = Keys::generate().public_key();
        let non_seal = EventBuilder::new(nostr_sdk::Kind::TextNote, "not a seal")
            .sign_with_keys(&Keys::generate())
            .unwrap();

        let err = gift_wrap_from_seal_with_pow(&receiver, &non_seal, Tags::new(), 0).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("kind"),
            "unexpected error: {err}"
        );
    }
}
