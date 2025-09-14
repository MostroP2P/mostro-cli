use anyhow::Result;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

use crate::parser::disputes::print_disputes_table;
use crate::util::{fetch_events_list, ListKind};

pub async fn execute_list_disputes(
    mostro_pubkey: PublicKey,
    mostro_keys: &Keys,
    trade_index: i64,
    pool: &SqlitePool,
    client: &Client,
) -> Result<()> {
    println!(
        "Requesting disputes from mostro pubId - {}",
        mostro_pubkey.clone()
    );

    // Get orders from relays
    let table_of_disputes = fetch_events_list(
        ListKind::Disputes,
        None,
        None,
        None,
        &mostro_pubkey,
        mostro_keys,
        trade_index,
        None,
        pool,
        client,
    )
    .await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
