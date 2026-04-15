use anyhow::Result;
use mostro_core::prelude::Message;
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    parser::common::{print_key_value, print_section_header},
    parser::dms::print_direct_messages,
    util::{fetch_events_list, Event, ListKind},
};

pub async fn execute_get_dm(
    since: &i64,
    admin: bool,
    from_user: &bool,
    ctx: &Context,
) -> Result<()> {
    print_section_header("📨 Fetch Direct Messages");
    print_key_value("👤", "Admin Mode", if admin { "Yes" } else { "No" });
    print_key_value("📤", "From User", if *from_user { "Yes" } else { "No" });
    print_key_value("⏰", "Since", &format!("{} minutes ago", since));
    print_key_value("💡", "Action", "Fetching direct messages...");
    println!();

    // Determine DM list to fetch (admin/user and from-user flag)
    let list_kind = match (admin, from_user) {
        (true, true) => ListKind::PrivateDirectMessagesUser,
        (true, false) => ListKind::DirectMessagesAdmin,
        (false, true) => ListKind::PrivateDirectMessagesUser,
        (false, false) => ListKind::DirectMessagesUser,
    };

    // Fetch the requested events
    let all_fetched_events =
        fetch_events_list(list_kind, None, None, None, ctx, Some(since)).await?;

    // Extract (Message, u64) tuples from Event::MessageTuple variants
    let mut dm_events: Vec<(Message, u64, PublicKey)> = Vec::new();
    for event in all_fetched_events {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    print_direct_messages(&dm_events, Some(ctx.mostro_pubkey)).await?;
    Ok(())
}
