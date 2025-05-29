use anyhow::Result;
use nostr_sdk::prelude::*;

use crate::pretty_table::print_disputes_table;
use crate::util::{fetch_events_list, ListKind};

pub async fn execute_list_disputes(mostro_key: PublicKey, client: &Client) -> Result<()> {
    println!(
        "Requesting disputes from mostro pubId - {}",
        mostro_key.clone()
    );

    // Get orders from relays
    let table_of_disputes =
        fetch_events_list(mostro_key, ListKind::Disputes, None, None, None, client).await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
