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

fn create_seven_days_filter(
    letter: Alphabet,
    value: String,
    pubkey: PublicKey,
    event_kind: u16,
) -> Result<Filter> {
    let since_time = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .ok_or(anyhow::anyhow!("Failed to get since days ago"))?
        .timestamp() as u64;
    let timestamp = Timestamp::from(since_time);
    Ok(Filter::new()
        .author(pubkey)
        .limit(50)
        .since(timestamp)
        .custom_tag(SingleLetterTag::lowercase(letter), value)
        .kind(nostr_sdk::Kind::Custom(event_kind)))
}

pub fn create_filter(
    list_kind: ListKind,
    pubkey: PublicKey,
    since: Option<&i64>,
) -> Result<Filter> {
    match list_kind {
        ListKind::Orders => create_seven_days_filter(
            Alphabet::Z,
            "order".to_string(),
            pubkey,
            NOSTR_ORDER_EVENT_KIND,
        ),
        ListKind::Disputes => create_seven_days_filter(
            Alphabet::Z,
            "dispute".to_string(),
            pubkey,
            NOSTR_DISPUTE_EVENT_KIND,
        ),
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

/// Fetch the Mostro instance's kind-38385 info event and read a tag whose
/// first slot equals `tag_name`. Returns the second slot as a string.
///
/// Best-effort: any relay error or missing tag degrades to `None` so callers
/// keep working against older daemons that don't publish the tag.
/// Pure: read the second slot of the first tag whose first slot equals
/// `tag_name`. Split out so the kind-38385 tag-parsing logic stays unit
/// testable without spinning up a relay.
fn read_info_tag_from_event(event: &nostr_sdk::Event, tag_name: &str) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(String::as_str) == Some(tag_name) {
            slice.get(1).cloned()
        } else {
            None
        }
    })
}

async fn fetch_info_tag(ctx: &crate::cli::Context, tag_name: &str) -> Option<String> {
    let filter = Filter::new()
        .author(ctx.mostro_pubkey)
        .kind(nostr_sdk::Kind::Custom(NOSTR_INFO_EVENT_KIND));

    let events = ctx
        .client
        .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
        .await
        .ok()?;

    // kind-38385 is replaceable, but pick the newest revision by `created_at`
    // explicitly: a lagging relay (or several relays at once) can still surface
    // an older copy.
    let event = events.iter().max_by_key(|e| e.created_at)?;
    read_info_tag_from_event(event, tag_name)
}

/// Fetch the Mostro instance's kind-38385 info event and read the
/// `bond_payout_claim_window_days` tag.
///
/// Returns `None` when the node publishes no info event, the tag is absent
/// (older daemon or bonds disabled), or the value can't be parsed. Used to
/// render the forfeit deadline for an `add-bond-invoice` request locally, per
/// the protocol's "Bond payout invoice" / "Other events" docs. Best-effort:
/// any relay error degrades to `None` rather than failing the caller.
pub async fn fetch_bond_claim_window_days(ctx: &crate::cli::Context) -> Option<i64> {
    // A stale claim window would render the wrong, very user-facing forfeit
    // deadline — same newest-revision caveat as the rest of the info event.
    fetch_info_tag(ctx, "bond_payout_claim_window_days")
        .await
        .and_then(|v| v.parse::<i64>().ok())
}

/// Fetch the Mostro instance's required NIP-13 proof-of-work difficulty from
/// the kind-38385 info event (`["pow", "<bits>"]` tag).
///
/// Returns `None` when the daemon doesn't publish the tag (older versions),
/// when the value is unparseable, or when the info event can't be fetched.
/// Used by [`crate::util::messaging::wait_for_dm`] to distinguish a real
/// timeout from a silent PoW rejection — see `docs/pow_error_handling.md`.
pub async fn fetch_required_pow(ctx: &crate::cli::Context) -> Option<u8> {
    fetch_info_tag(ctx, "pow")
        .await
        .and_then(|v| v.parse::<u8>().ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_info_event(keys: &Keys, tags: Vec<Tag>) -> nostr_sdk::Event {
        EventBuilder::new(nostr_sdk::Kind::Custom(NOSTR_INFO_EVENT_KIND), "")
            .tags(tags)
            .sign(keys)
            .await
            .expect("sign info event")
    }

    fn pow_tag(value: &str) -> Tag {
        Tag::parse(["pow", value]).expect("build pow tag")
    }

    #[tokio::test]
    async fn read_info_tag_finds_pow_value() {
        let keys = Keys::generate();
        let event = make_info_event(
            &keys,
            vec![
                Tag::parse(["fee", "0.006"]).unwrap(),
                pow_tag("12"),
                Tag::parse(["fiat_currencies_accepted", "USD,EUR"]).unwrap(),
            ],
        )
        .await;
        assert_eq!(read_info_tag_from_event(&event, "pow"), Some("12".into()));
    }

    #[tokio::test]
    async fn read_info_tag_returns_none_when_missing() {
        let keys = Keys::generate();
        let event = make_info_event(&keys, vec![Tag::parse(["fee", "0.006"]).unwrap()]).await;
        assert_eq!(read_info_tag_from_event(&event, "pow"), None);
    }

    #[tokio::test]
    async fn pow_tag_parses_as_u8() {
        // u8 parse is what fetch_required_pow chains after the helper.
        // Lock in that the daemon's stringified u8 round-trips cleanly,
        // and that garbage values degrade to None.
        let parse = |s: &str| s.parse::<u8>().ok();
        assert_eq!(parse("12"), Some(12));
        assert_eq!(parse("0"), Some(0));
        assert_eq!(parse("nope"), None);
        // Out of range for u8 → None, which is the right "ignore this
        // weird value, fall back to generic timeout" behavior.
        assert_eq!(parse("999"), None);
    }
}
