use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::db::User;
use crate::parser::{parse_dispute_events, parse_dm_events, parse_orders_events};
use crate::util::messaging::get_admin_keys;

pub const FETCH_EVENTS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const FAKE_SINCE: i64 = 2880;

use super::types::{Event, ListKind};

fn create_fake_timestamp() -> Result<Timestamp> {
    let fake_since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(FAKE_SINCE))
        .ok_or(anyhow::anyhow!("Failed to get fake since time"))?
        .timestamp() as u64;
    Ok(Timestamp::from(fake_since_time))
}

fn create_seven_days_filter(kind: u16, pubkey: PublicKey) -> Result<Filter> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .ok_or(anyhow::anyhow!("Failed to get since days ago"))?
        .timestamp() as u64;
    let timestamp = Timestamp::from(since_time);
    Ok(Filter::new()
        .author(pubkey)
        .limit(50)
        .since(timestamp)
        .kind(nostr_sdk::Kind::Custom(kind)))
}

pub fn create_filter(
    list_kind: ListKind,
    pubkey: PublicKey,
    since: Option<&i64>,
) -> Result<Filter> {
    match list_kind {
        ListKind::Orders => create_seven_days_filter(NOSTR_ORDER_EVENT_KIND, pubkey),
        ListKind::Disputes => create_seven_days_filter(NOSTR_DISPUTE_EVENT_KIND, pubkey),
        ListKind::DirectMessagesAdmin | ListKind::DirectMessagesUser => {
            let fake_timestamp = create_fake_timestamp()?;
            Ok(Filter::new()
                .kind(nostr_sdk::Kind::GiftWrap)
                .pubkey(pubkey)
                .since(fake_timestamp))
        }
        ListKind::PrivateDirectMessagesUser => {
            let since = if let Some(mins) = since {
                chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(*mins))
                    .unwrap()
                    .timestamp()
            } else {
                chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::minutes(30))
                    .unwrap()
                    .timestamp()
            } as u64;
            Ok(Filter::new()
                .kind(nostr_sdk::Kind::PrivateDirectMessage)
                .pubkey(pubkey)
                .since(Timestamp::from(since)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_events_list(
    list_kind: ListKind,
    status: Option<Status>,
    currency: Option<String>,
    kind: Option<mostro_core::order::Kind>,
    ctx: &crate::cli::Context,
    since: Option<&i64>,
) -> Result<Vec<Event>> {
    match list_kind {
        ListKind::Orders => {
            let filters = create_filter(list_kind, ctx.mostro_pubkey, None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
                .await?;
            let orders = parse_orders_events(fetched_events, currency, status, kind);
            Ok(orders.into_iter().map(Event::SmallOrder).collect())
        }
        ListKind::DirectMessagesAdmin => {
            // Get admin keys
            let admin_keys = get_admin_keys(ctx)?;
            // Create filter
            let filters = create_filter(list_kind, admin_keys.public_key(), None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
                .await?;
            let direct_messages_mostro = parse_dm_events(fetched_events, admin_keys, since).await;
            Ok(direct_messages_mostro
                .into_iter()
                .map(|(message, timestamp, sender_pubkey)| {
                    Event::MessageTuple(Box::new((message, timestamp, sender_pubkey)))
                })
                .collect())
        }
        ListKind::PrivateDirectMessagesUser => {
            let mut direct_messages: Vec<(Message, u64, PublicKey)> = Vec::new();
            for index in 1..=ctx.trade_index {
                let trade_key = User::get_trade_keys(&ctx.pool, index).await?;
                let filter = create_filter(
                    ListKind::PrivateDirectMessagesUser,
                    trade_key.public_key(),
                    None,
                )?;
                let fetched_user_messages = ctx
                    .client
                    .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
                    .await?;
                let direct_messages_for_trade_key =
                    parse_dm_events(fetched_user_messages, &trade_key, since).await;
                // Extend the direct messages
                direct_messages.extend(direct_messages_for_trade_key);
            }
            Ok(direct_messages
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::DirectMessagesUser => {
            let mut direct_messages: Vec<(Message, u64, PublicKey)> = Vec::new();
            for index in 1..=ctx.trade_index {
                let trade_key = User::get_trade_keys(&ctx.pool, index).await?;
                let filter =
                    create_filter(ListKind::DirectMessagesUser, trade_key.public_key(), None)?;
                let fetched_user_messages = ctx
                    .client
                    .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
                    .await?;
                let direct_messages_for_trade_key =
                    parse_dm_events(fetched_user_messages, &trade_key, since).await;
                // Extend the direct messages
                direct_messages.extend(direct_messages_for_trade_key);
            }
            Ok(direct_messages
                .into_iter()
                .map(|t| Event::MessageTuple(Box::new(t)))
                .collect())
        }
        ListKind::Disputes => {
            let filters = create_filter(list_kind, ctx.mostro_pubkey, None)?;
            let fetched_events = ctx
                .client
                .fetch_events(filters, FETCH_EVENTS_TIMEOUT)
                .await?;
            let disputes = parse_dispute_events(fetched_events);
            Ok(disputes.into_iter().map(Event::Dispute).collect())
        }
    }
}
