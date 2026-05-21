use anyhow::Result;
use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    parser::common::{print_key_value, print_section_header},
    parser::dms::print_direct_messages,
    util::{fetch_bond_claim_window_days, fetch_events_list, Event, ListKind},
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

    // Only hit the relay for the node's claim window when an inbound bond
    // payout request is actually present, so the common get-dm path stays cheap.
    let has_bond_payout_request = dm_events.iter().any(|(message, _, _)| {
        let inner = message.get_inner_message_kind();
        inner.action == Action::AddBondInvoice
            && matches!(inner.payload, Some(Payload::BondPayoutRequest(_)))
    });
    let claim_window_days = if has_bond_payout_request {
        fetch_bond_claim_window_days(ctx).await
    } else {
        None
    };

    print_direct_messages(&dm_events, Some(ctx.mostro_pubkey), claim_window_days).await?;
    Ok(())
}
