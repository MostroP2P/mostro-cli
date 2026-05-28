use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use mostro_core::prelude::*;
use nip44::v2::{encrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;
use std::env::var;

use crate::cli::Context;
use crate::parser::dms::print_commands_results;
use crate::parser::parse_dm_events;
use crate::util::events::FETCH_EVENTS_TIMEOUT;

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
        inner_event.created_at.as_secs() as i64,
        inner_event.pubkey,
    ))
}

/// Fetch gift wrap events addressed to a specific shared key's public key,
/// decrypt each with the shared key, and return (content, timestamp, sender_pubkey).
pub async fn fetch_gift_wraps_for_shared_key(
    client: &Client,
    shared_keys: &Keys,
) -> Result<Vec<(String, i64, PublicKey)>> {
    let now = Timestamp::now().as_secs();
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

/// Internal: wrap a Mostro `Message` via [`wrap_message`] and publish it.
///
/// Follows the mostro-core 0.10 dual-key split: `identity_keys` sign the
/// seal (long-lived reputation binding), `trade_keys` author the rumor and
/// produce the inner tuple signature. Pass the same `Keys` for both to
/// opt into full-privacy mode.
async fn publish_gift_wrap(
    client: &Client,
    identity_keys: &Keys,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    message: &Message,
    opts: WrapOptions,
) -> Result<()> {
    let event = wrap_message(message, identity_keys, trade_keys, *receiver_pubkey, opts)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wrap message: {e}"))?;
    client.send_event(&event).await?;
    Ok(())
}

/// Send a plain-text DM wrapped as a NIP-59 Gift Wrap.
///
/// The wrap uses `signed = false` so the inner rumor carries `(Message, None)`.
/// `identity_keys` sign the seal and `trade_keys` author the rumor; admin
/// flows that do not rotate trade keys should pass the admin keys for both.
pub async fn send_plain_text_dm(
    client: &Client,
    identity_keys: &Keys,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    text: &str,
) -> Result<()> {
    let pow = parse_pow_env()?;
    let dm_message = Message::new_dm(
        None,
        None,
        Action::SendDm,
        Some(Payload::TextMessage(text.to_string())),
    );
    let opts = WrapOptions {
        pow,
        expiration: None,
        signed: false,
    };
    publish_gift_wrap(
        client,
        identity_keys,
        trade_keys,
        receiver_pubkey,
        &dm_message,
        opts,
    )
    .await
}

/// Upper bound on the post-timeout PoW probe inside [`wait_for_dm`]. Kept
/// well below `FETCH_EVENTS_TIMEOUT` so a slow/unreachable relay can't
/// double the user-visible wait — if the kind-38385 info event isn't back
/// within this budget we fall through to [`WaitForDmTimeout`] instead.
const POW_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Distinguishable error returned by [`wait_for_dm`] when no reply arrives
/// within [`FETCH_EVENTS_TIMEOUT`].
///
/// Most callers `?`-propagate it like any other error, but flows where "no
/// reply" is the happy path (e.g. `add-bond-invoice`, where Mostro pays the
/// invoice without acking over Nostr) can detect it via
/// `downcast_ref::<WaitForDmTimeout>()` and avoid misreporting genuine
/// subscribe/send/transport failures as success.
#[derive(Debug)]
pub struct WaitForDmTimeout;

impl std::fmt::Display for WaitForDmTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Timeout waiting for DM or gift wrap event")
    }
}

impl std::error::Error for WaitForDmTimeout {}

/// Returned by [`wait_for_dm`] when the wait timed out *and* the Mostro
/// instance's kind-38385 info event advertises a NIP-13 PoW difficulty
/// strictly above what the client sent. Lets callers (and tests)
/// distinguish a silent PoW rejection — the daemon dropped the event after
/// the relay accepted it — from a generic timeout.
///
/// See `docs/pow_error_handling.md` for the design rationale.
#[derive(Debug)]
pub struct PowRequirementUnmet {
    pub required: u8,
    pub configured: u8,
}

impl std::fmt::Display for PowRequirementUnmet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This Mostro instance requires NIP-13 proof of work of {} bits, \
             but the client sent the event with {} bits. \
             Re-run with `--pow {}` or set `POW={}` and try again.",
            self.required, self.configured, self.required, self.required
        )
    }
}

impl std::error::Error for PowRequirementUnmet {}

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

    // Kick off the PoW probe concurrently with the DM wait. By running the
    // kind-38385 lookup alongside the 15s `FETCH_EVENTS_TIMEOUT` instead of
    // *after* it, the timeout branch doesn't pay a second sequential
    // `fetch_events` round-trip — by then the probe has typically already
    // returned. `JoinHandle` lets us `abort()` the probe cheaply on the happy
    // path (DM arrives in time) without leaking the task.
    let pow_probe = tokio::spawn(super::events::fetch_required_pow_with(
        ctx.client.clone(),
        ctx.mostro_pubkey,
    ));

    // Wait for the DM or gift wrap event
    let waited = tokio::time::timeout(super::events::FETCH_EVENTS_TIMEOUT, async move {
        loop {
            match notifications.recv().await {
                Ok(RelayPoolNotification::Event { event, .. }) => return Ok(*event),
                Ok(_) => continue,
                Err(e) => {
                    return Err(anyhow::anyhow!("Error receiving notification: {:?}", e));
                }
            }
        }
    })
    .await;

    // Keep a genuine timeout (the only "no reply" outcome) distinguishable from
    // a notification-channel error so callers can treat them differently.
    let event = match waited {
        Ok(inner) => {
            // Happy path: DM arrived. Cancel the probe; the answer is no
            // longer needed and we don't want a stray relay request lingering.
            pow_probe.abort();
            inner?
        }
        Err(_elapsed) => {
            // mostrod silently drops events whose outer GiftWrap doesn't meet
            // its NIP-13 PoW requirement (relay accepts → daemon discards →
            // no reply ever comes). The probe has already been running for
            // `FETCH_EVENTS_TIMEOUT` alongside the wait, so it is almost
            // certainly done. Cap the await with `POW_PROBE_TIMEOUT` as a
            // safety net so a pathological relay can't keep us hanging — if
            // the probe isn't back by then, fall through to the generic
            // timeout error instead of waiting any longer.
            let probe_result = tokio::time::timeout(POW_PROBE_TIMEOUT, pow_probe).await;
            if let Ok(Ok(Some(required))) = probe_result {
                let configured = parse_pow_env().unwrap_or(0);
                if required > configured {
                    return Err(PowRequirementUnmet {
                        required,
                        configured,
                    }
                    .into());
                }
            }
            return Err(WaitForDmTimeout.into());
        }
    };

    let mut events = Events::default();
    events.insert(event);
    Ok(events)
}

fn parse_pow_env() -> Result<u8> {
    var("POW")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<u8>()
        .map_err(|e| anyhow::anyhow!("Failed to parse POW: {}", e))
}

fn parse_secret_env() -> Result<bool> {
    var("SECRET")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .map_err(|e| anyhow::anyhow!("Failed to parse SECRET: {}", e))
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

/// Send a Mostro protocol message to `receiver_pubkey`.
///
/// mostro-core 0.10 splits the NIP-59 pipeline across two keys:
///
/// * `identity_keys` sign the seal (kind 13). Long-lived per user — the
///   key the Mostro node uses to attach reputation. Admin flows pass the
///   admin keys here; identity-scoped requests (restore, last trade index)
///   pass the account's identity keys.
/// * `trade_keys` author the rumor (kind 1) and produce the inner tuple
///   signature when `signed = true`. Rotated per order for user flows,
///   equal to `identity_keys` for full-privacy mode and for flows that
///   don't bind to a specific trade (admin, restore, last trade index).
///
/// For NIP-17 `PrivateDirectMessage` traffic (`to_user = true`), kind 14
/// is signed directly by `trade_keys` — identity is irrelevant because
/// there is no seal.
///
/// Respects the `POW` and `SECRET` env vars: PoW is mined on the outer
/// wrap (or kind-14 event), and `SECRET=true` flips the inner tuple to
/// unsigned. Gift wraps go through [`mostro_core::prelude::wrap_message`].
pub async fn send_dm(
    client: &Client,
    identity_keys: &Keys,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    payload: String,
    expiration: Option<Timestamp>,
    to_user: bool,
) -> Result<()> {
    let pow = parse_pow_env()?;

    if to_user {
        let event = create_private_dm_event(trade_keys, receiver_pubkey, payload, pow).await?;
        client.send_event(&event).await?;
        return Ok(());
    }

    let message = Message::from_json(&payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize message: {e}"))?;
    let private = parse_secret_env()?;
    let opts = WrapOptions {
        pow,
        expiration,
        signed: !private,
    };

    publish_gift_wrap(
        client,
        identity_keys,
        trade_keys,
        receiver_pubkey,
        &message,
        opts,
    )
    .await
}

pub async fn print_dm_events(
    recv_event: Events,
    request_id: u64,
    ctx: &crate::cli::Context,
    order_trade_keys: Option<&Keys>,
) -> Result<()> {
    let trade_keys = order_trade_keys.unwrap_or(&ctx.trade_keys);
    let messages = parse_dm_events(recv_event, trade_keys, None).await;
    let (message, _, _) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("No response received from Mostro"))?;
    let inner = message.get_inner_message_kind();

    match validate_response(message, Some(request_id)) {
        Ok(()) => {}
        // `mostro_core::nip59::validate_response` intentionally leaves
        // `NewOrder` out of the unsolicited-push allow-list. Preserve the
        // CLI's legacy tolerance so a child order published after a range
        // trade (no `request_id`) still gets printed.
        Err(_) if inner.request_id.is_none() && inner.action == Action::NewOrder => {}
        Err(e) => return Err(anyhow::anyhow!("Unexpected response from Mostro: {e}")),
    }

    print_commands_results(inner, ctx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_requirement_unmet_display_mentions_required_and_configured() {
        let rendered = PowRequirementUnmet {
            required: 12,
            configured: 4,
        }
        .to_string();
        assert!(
            rendered.contains("12") && rendered.contains("4"),
            "message should surface both PoW values, got: {rendered}",
        );
        assert!(
            rendered.contains("proof of work"),
            "message should call out PoW explicitly, got: {rendered}",
        );
        assert!(
            rendered.contains("--pow") && rendered.contains("POW="),
            "message should hint at both the CLI flag and env var, got: {rendered}",
        );
    }

    #[test]
    fn pow_requirement_unmet_is_downcastable_through_anyhow() {
        // The add-bond-invoice flow only catches WaitForDmTimeout via
        // downcast_ref::<WaitForDmTimeout>(). Verify our new error stays
        // distinct so PoW failures don't get misreported as a successful
        // bond-invoice submission.
        let err: anyhow::Error = PowRequirementUnmet {
            required: 8,
            configured: 0,
        }
        .into();
        assert!(err.downcast_ref::<PowRequirementUnmet>().is_some());
        assert!(err.downcast_ref::<WaitForDmTimeout>().is_none());
    }

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

    fn sample_protocol_message(request_id: Option<u64>) -> Message {
        Message::new_order(
            None,
            request_id,
            Some(1),
            Action::NewOrder,
            Some(Payload::TextMessage("hi".to_string())),
        )
    }

    // Cryptographic correctness of wrap_message / unwrap_message lives in
    // mostro-core. These tests only exercise the CLI wiring: that the
    // Message we hand to send_dm survives a wrap→unwrap roundtrip and that
    // our WrapOptions knobs (signed, pow) reach the outer event.

    #[tokio::test]
    async fn send_dm_gift_wrap_roundtrips_via_unwrap_message() {
        let identity_keys = Keys::generate();
        let trade_keys = Keys::generate();
        let mostro_keys = Keys::generate();
        let message = sample_protocol_message(Some(42));

        let event = wrap_message(
            &message,
            &identity_keys,
            &trade_keys,
            mostro_keys.public_key(),
            WrapOptions::default(),
        )
        .await
        .expect("wrap");

        assert_eq!(event.kind, nostr_sdk::Kind::GiftWrap);

        let unwrapped = unwrap_message(&event, &mostro_keys)
            .await
            .expect("unwrap result")
            .expect("addressed to mostro_keys");

        assert_eq!(unwrapped.sender, trade_keys.public_key());
        assert_eq!(unwrapped.identity, identity_keys.public_key());
        assert_eq!(
            unwrapped.message.as_json().unwrap(),
            message.as_json().unwrap()
        );
        assert!(
            unwrapped.signature.is_some(),
            "default WrapOptions has signed=true",
        );
    }

    #[tokio::test]
    async fn full_privacy_mode_identity_equals_trade() {
        let trade_keys = Keys::generate();
        let mostro_keys = Keys::generate();
        let message = sample_protocol_message(Some(7));

        let event = wrap_message(
            &message,
            &trade_keys,
            &trade_keys,
            mostro_keys.public_key(),
            WrapOptions::default(),
        )
        .await
        .expect("wrap");

        let unwrapped = unwrap_message(&event, &mostro_keys).await.unwrap().unwrap();
        assert_eq!(unwrapped.sender, trade_keys.public_key());
        assert_eq!(unwrapped.identity, unwrapped.sender);
    }

    #[tokio::test]
    async fn secret_env_semantics_drop_inner_signature() {
        let identity_keys = Keys::generate();
        let trade_keys = Keys::generate();
        let mostro_keys = Keys::generate();

        let event = wrap_message(
            &sample_protocol_message(Some(1)),
            &identity_keys,
            &trade_keys,
            mostro_keys.public_key(),
            WrapOptions {
                signed: false,
                ..Default::default()
            },
        )
        .await
        .expect("wrap");

        let unwrapped = unwrap_message(&event, &mostro_keys).await.unwrap().unwrap();
        assert!(unwrapped.signature.is_none());
    }

    #[tokio::test]
    async fn wrap_message_respects_pow_option() {
        let identity_keys = Keys::generate();
        let trade_keys = Keys::generate();
        let mostro_keys = Keys::generate();
        let pow = 4;

        let event = wrap_message(
            &sample_protocol_message(None),
            &identity_keys,
            &trade_keys,
            mostro_keys.public_key(),
            WrapOptions {
                pow,
                ..Default::default()
            },
        )
        .await
        .expect("wrap");

        assert!(event_meets_pow(&event, pow), "PoW not met");
    }

    #[tokio::test]
    async fn wrong_keys_yield_none_on_unwrap() {
        let identity_keys = Keys::generate();
        let trade_keys = Keys::generate();
        let mostro_keys = Keys::generate();
        let stranger = Keys::generate();

        let event = wrap_message(
            &sample_protocol_message(Some(1)),
            &identity_keys,
            &trade_keys,
            mostro_keys.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();

        let result = unwrap_message(&event, &stranger).await.expect("no error");
        assert!(result.is_none());
    }
}
