use anyhow::Result;
use mostro_core::prelude::Message;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

use crate::{
    parser::dms::print_direct_messages,
    util::{fetch_events_list, Event, ListKind},
};

pub async fn execute_get_dm(
    _since: &i64,
    trade_index: i64,
    mostro_keys: &Keys,
    client: &Client,
    admin: bool,
    pool: &SqlitePool,
) -> Result<()> {
    // Fetch the requested events
    let all_fetched_events = if !admin {
        fetch_events_list(
            ListKind::DirectMessagesUser,
            None,
            None,
            None,
            mostro_keys,
            trade_index,
            pool,
            client,
        )
        .await?
        // all_fetched_events.extend(fetched_events);
    } else {
        fetch_events_list(
            ListKind::DirectMessagesMostro,
            None,
            None,
            None,
            mostro_keys,
            trade_index,
            pool,
            client,
        )
        .await?
        // all_fetched_events.extend(fetched_events);
    };

    // Extract (Message, u64) tuples from Event::MessageTuple variants
    let mut dm_events: Vec<(Message, u64)> = Vec::new();
    for event in all_fetched_events {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    print_direct_messages(&dm_events, pool).await?;
    Ok(())
}
