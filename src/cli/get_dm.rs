use anyhow::Result;
use mostro_core::prelude::Message;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

use crate::{
    cli::Context, parser::dms::print_direct_messages, util::{fetch_events_list, Event, ListKind}
};

pub async fn execute_get_dm(
    since: Option<&i64>,
    admin: bool,
    from_user: &bool,
    ctx :&Context
) -> Result<()> {
    // Get the list kind
    let list_kind = match (admin, from_user) {
        (true, true) => ListKind::PrivateDirectMessagesUser,
        (true, false) => ListKind::DirectMessagesAdmin,
        (false, true) => ListKind::PrivateDirectMessagesUser,
        (false, false) => ListKind::DirectMessagesUser,
    };

    // Fetch the requested events
    let all_fetched_events = {
        fetch_events_list(
            list_kind,
            None,
            None,
            None,
            &ctx,
            since
        )
        .await?
    };

    // Extract (Message, u64) tuples from Event::MessageTuple variants
    let mut dm_events: Vec<(Message, u64)> = Vec::new();
    for event in all_fetched_events {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    print_direct_messages(&dm_events, &ctx.pool).await?;
    Ok(())
}
