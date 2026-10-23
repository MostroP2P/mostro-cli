use anyhow::Result;
use nostr_sdk::prelude::*;
use std::env::var;
use std::sync::Once;
use std::time::Duration;

use crate::msg::{ephemeral_client, relay_entries};

/// rustls 0.23 cannot auto-pick a process CryptoProvider when more than one
/// backend feature is in the graph. Pin `ring` (same as tonic `tls-ring` and
/// nostr-sdk's default) before any TLS handshake.
pub fn install_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Upper bound on how long [`connect_nostr`] blocks waiting for the relay
/// handshakes to complete. `Client::connect` only *spawns* background
/// connection tasks and returns immediately, so without this wait the very
/// next network op (the transport probe, then `subscribe` + `send_dm`) races
/// the still-in-progress handshake. On a fast/local relay this returns in
/// milliseconds; it only blocks the full budget when a relay is unreachable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn connect_nostr() -> Result<Client> {
    let relays = var("RELAYS").map_err(|_| anyhow::anyhow!("RELAYS is not set"))?;
    let relays: Vec<&str> = relay_entries(&relays).collect();
    if relays.is_empty() {
        return Err(anyhow::anyhow!("RELAYS is not set"));
    }
    let client = ephemeral_client();
    for r in relays.into_iter() {
        client.add_relay(r).await?;
    }
    // `connect` is fire-and-forget unless we wait: without this, the very
    // next network op (the transport probe, then `subscribe` + `send_dm`) races
    // the still-in-progress handshake. On a fast/local relay this returns in
    // milliseconds; it only blocks the full budget when a relay is unreachable.
    client.connect().and_wait(CONNECT_TIMEOUT).await;
    Ok(client)
}
