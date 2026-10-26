//! Order-book query, decoding and latest-state reduction.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Duration, Utc};
use log::{error, info};
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use nostr_sdk::prelude::{Event, Filter, Kind, PublicKey, SingleLetterTag, Timestamp};

use crate::nip33::order_from_tags;
use crate::outcome::ErrorCode;

const PER_RELAY_LIMIT: usize = 50;

/// Events of `kind` by `author` tagged `z`, from seven days before `now`.
pub(crate) fn seven_day_filter(
    z: &str,
    author: PublicKey,
    kind: u16,
    now: DateTime<Utc>,
) -> Result<Filter, ErrorCode> {
    let since = now
        .checked_sub_signed(Duration::days(7))
        .ok_or(ErrorCode::InternalError)?
        .timestamp() as u64;
    Ok(Filter::new()
        .author(author)
        .limit(PER_RELAY_LIMIT)
        .since(Timestamp::from(since))
        .custom_tag(SingleLetterTag::LOWERCASE_Z, z)
        .kind(Kind::Custom(kind)))
}

/// The latest state of each order, newest first, kept only when it matches
/// every given filter. Filtering after reduction keeps a stale state from
/// resurfacing an order whose latest state does not match.
pub(crate) fn reduce_orders(
    events: &BTreeSet<Event>,
    currency: Option<&str>,
    status: Option<Status>,
    side: Option<OrderKind>,
) -> Vec<SmallOrder> {
    latest_orders(events)
        .into_iter()
        .filter(|order| status.is_none_or(|status| order.status == Some(status)))
        .filter(|order| currency.is_none_or(|currency| order.fiat_code == currency))
        .filter(|order| {
            side.as_ref()
                .is_none_or(|side| order.kind.as_ref() == Some(side))
        })
        .collect()
}

/// The set iterates newest first and, on equal times, lowest event id first,
/// so the first decodable event of an order is its latest state.
fn latest_orders(events: &BTreeSet<Event>) -> Vec<SmallOrder> {
    let mut seen = HashSet::new();
    events
        .iter()
        .filter_map(decode_order)
        .filter(|order| order.id.is_some_and(|id| seen.insert(id)))
        .collect()
}

/// Decode an order event, skipping it when it has no order id or side.
fn decode_order(event: &Event) -> Option<SmallOrder> {
    let mut order = match order_from_tags(event.tags.clone()) {
        Ok(order) => order,
        Err(e) => {
            error!("{e:?}");
            return None;
        }
    };
    if order.id.is_none() {
        info!("Order ID is none");
        return None;
    }
    if order.kind.is_none() {
        info!("Order kind is none");
        return None;
    }
    order.created_at = Some(i64::try_from(event.created_at.as_secs()).unwrap_or(i64::MAX));
    Some(order)
}
