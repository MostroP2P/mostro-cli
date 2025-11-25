use anyhow::Result;
use nostr_sdk::prelude::*;
use std::env::var;

pub async fn connect_nostr() -> Result<Client> {
    let my_keys = Keys::generate();

    let relays = var("RELAYS").expect("RELAYS is not set");
    let relays = relays.split(',').collect::<Vec<&str>>();
    let client = Client::new(my_keys);
    for r in relays.into_iter() {
        client.add_relay(r).await?;
    }
    client.connect().await;
    Ok(client)
}
