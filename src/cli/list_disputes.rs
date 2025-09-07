use anyhow::Result;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

use crate::parser::disputes::print_disputes_table;
use crate::util::{fetch_events_list, ListKind};

pub async fn execute_list_disputes(
    mostro_key: PublicKey,
    mostro_keys: &Keys,
    trade_index: i64,
    pool: &SqlitePool,
    client: &Client,
) -> Result<()> {
    println!(
        "Requesting disputes from mostro pubId - {}",
        mostro_key.clone()
    );

    // Get orders from relays
    let table_of_disputes = fetch_events_list(crate::util::FetchEventsParams {
        list_kind: ListKind::Disputes,
        status: None,
        currency: None,
        kind: None,
        mostro_pubkey: mostro_key,
        mostro_keys,
        trade_index,
        pool,
        client,
    })
    .await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
