use anyhow::Result;
use nostr_sdk::prelude::*;
use std::env::var;
use std::time::Duration;

/// Upper bound on how long [`connect_nostr`] blocks waiting for the relay
/// handshakes to complete. `Client::connect` only *spawns* background
/// connection tasks and returns immediately, so without this wait the very
/// next network op (the transport probe, then `subscribe` + `send_dm`) races
/// the still-in-progress handshake. On a fast/local relay this returns in
/// milliseconds; it only blocks the full budget when a relay is unreachable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = Keys::generate();

    let relays = var("RELAYS").map_err(|_| anyhow::anyhow!("RELAYS is not set"))?;
    // Trim each entry and drop empty ones so stray whitespace or trailing commas
    // (e.g. `RELAYS="wss://a, ,wss://b,"`) don't reach `add_relay` and fail.
    let relays = relays
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect::<Vec<&str>>();
    if relays.is_empty() {
        return Err(anyhow::anyhow!("RELAYS is not set"));
    }
    let client = Client::builder()
        .authenticator(SignerAuthenticator::new(my_keys))
        .build();
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
