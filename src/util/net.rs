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

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    let client = Client::new(my_keys);
    for r in relays.into_iter() {
        client.add_relay(r).await?;
    }
    client.connect().await;
    // `connect` is fire-and-forget: it doesn't wait for the sockets to come up.
    // Block until the relays are actually connected so the immediately
    // following transport auto-detection, subscription and DM publish don't
    // race the handshake — otherwise a fast (e.g. local) Mostro can reply
    // before our subscription lands and, with `limit(0)`, the live-only
    // subscription never sees the stored reply → `wait_for_dm` times out.
    client.wait_for_connection(CONNECT_TIMEOUT).await;
    Ok(client)
}
