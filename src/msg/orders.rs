//! Order-book query, decoding and latest-state reduction.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Duration, Utc};
use log::{debug, info};
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::NOSTR_ORDER_EVENT_KIND;
use nostr_sdk::prelude::{Event, Filter, Kind, PublicKey, SingleLetterTag, Timestamp};

use crate::nip33::order_from_tags;
use crate::outcome::ErrorCode;

pub(crate) use crate::outcome::order::{ORDER_BOOK_LIMIT, PER_RELAY_LIMIT};

/// Events of `kind` by `author` tagged `z`, from seven days before `now`.
pub(crate) fn seven_day_filter(
    z: &str,
    author: PublicKey,
    kind: u16,
    now: DateTime<Utc>,
) -> Result<Filter, ErrorCode> {
    Ok(Filter::new()
        .author(author)
        .limit(PER_RELAY_LIMIT)
        .since(seven_days_before(now)?)
        .custom_tag(SingleLetterTag::LOWERCASE_Z, z)
        .kind(Kind::Custom(kind)))
}

/// The order book of `author` in one `status`, from seven days before `now`:
/// the relay filters the status, so other statuses do not use up the limit.
pub(crate) fn order_book_filter(
    author: PublicKey,
    status: Status,
    now: DateTime<Utc>,
) -> Result<Filter, ErrorCode> {
    Ok(
        seven_day_filter("order", author, NOSTR_ORDER_EVENT_KIND, now)?
            .limit(ORDER_BOOK_LIMIT)
            .custom_tag(SingleLetterTag::LOWERCASE_S, status.to_string()),
    )
}

fn seven_days_before(now: DateTime<Utc>) -> Result<Timestamp, ErrorCode> {
    let since = now
        .checked_sub_signed(Duration::days(7))
        .ok_or(ErrorCode::InternalError)?
        .timestamp();
    let since = u64::try_from(since).map_err(|_| ErrorCode::InternalError)?;
    Ok(Timestamp::from(since))
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
    let Ok(mut order) = order_from_tags(event.tags.clone()) else {
        // Fixed text: tag values are untrusted relay data.
        debug!("Order tags could not be decoded");
        return None;
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, to_value};

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, 0, 0, 0).unwrap()
    }

    #[test]
    fn the_order_book_filter_asks_the_relay_for_one_status() {
        let author = Keys::generate().public_key();
        for (status, tag) in [
            (Status::Pending, "pending"),
            (Status::InProgress, "in-progress"),
            (Status::Canceled, "canceled"),
        ] {
            let filter = to_value(order_book_filter(author, status, now()).unwrap()).unwrap();
            assert_eq!(filter["#s"], json!([tag]), "{filter}");
            assert_eq!(filter["#z"], json!(["order"]), "{filter}");
            assert_eq!(filter["limit"], ORDER_BOOK_LIMIT, "{filter}");
        }
    }

    #[test]
    fn a_window_starting_before_1970_is_an_internal_error() {
        let author = Keys::generate().public_key();
        let early = Utc.with_ymd_and_hms(1970, 1, 3, 0, 0, 0).unwrap();
        assert_eq!(
            order_book_filter(author, Status::Pending, early).unwrap_err(),
            ErrorCode::InternalError
        );
    }

    #[test]
    fn the_order_book_reads_300_events_from_the_same_window() {
        let author = Keys::generate().public_key();
        let book = order_book_filter(author, Status::Pending, now()).unwrap();
        let other = seven_day_filter("order", author, NOSTR_ORDER_EVENT_KIND, now()).unwrap();
        assert_eq!(ORDER_BOOK_LIMIT, 300);
        assert_eq!(book.since, other.since);
        assert_eq!(book.kinds, other.kinds);
        assert_eq!(book.authors, other.authors);
        assert_eq!(other.limit, Some(PER_RELAY_LIMIT));
    }
}
