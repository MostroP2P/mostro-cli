//! `order.list`: read the public order book.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mostro_core::prelude::NOSTR_ORDER_EVENT_KIND;
use nostr_sdk::prelude::{Event, PublicKey};

use crate::msg::{fetch_msg, reduce_orders, seven_day_filter, PublicReadConfig, ReadLimits};
use crate::outcome::order::{ListData, ListOrdersInput, Snapshot};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings};

pub const COMMAND: &str = "order.list";

pub type ListOutcome = Envelope<ListData, Snapshot>;

/// Read the public book and select orders matching the raw filter values.
///
/// Filters are validated before `config` is resolved, so invalid arguments
/// never read node configuration. The only network access is one bounded
/// `fetch_msg`; nothing is printed, persisted or retried.
pub async fn list_orders(
    side: Option<&str>,
    currency: Option<&str>,
    status: Option<&str>,
    config: impl FnOnce() -> Result<PublicReadConfig, ErrorCode>,
    limits: &ReadLimits,
    now: DateTime<Utc>,
) -> ListOutcome {
    let filters = match ListOrdersInput::parse(side, currency, status) {
        Ok(filters) => filters,
        Err(code) => return failed(code, Warnings::new(), None),
    };
    let config = match config() {
        Ok(config) => config,
        Err(code) => return failed(code, Warnings::new(), None),
    };
    let query = match seven_day_filter("order", config.mostro_pubkey(), NOSTR_ORDER_EVENT_KIND, now)
    {
        Ok(query) => query,
        Err(code) => return failed(code, Warnings::new(), None),
    };
    let fetched = match fetch_msg(&config, query, limits).await {
        Ok(fetched) => fetched,
        Err(failure) => {
            return failed(
                failure.code(),
                failure.warnings().clone(),
                failure.snapshot().cloned(),
            )
        }
    };

    let mut warnings = fetched.warnings().clone();
    let snapshot = Some(fetched.snapshot().clone());
    match select_orders(&config.mostro_pubkey(), filters, fetched.events()) {
        Ok((data, selection_warnings)) => {
            warnings.merge(&selection_warnings);
            Envelope::ok(COMMAND, data, warnings, ExecutionMeta::read(snapshot))
        }
        Err(code) => failed(code, warnings, snapshot),
    }
}

/// Reduce collected order events to their latest state per order id, apply
/// `filters`, and map the rows, newest event first.
pub fn select_orders(
    node_pubkey: &PublicKey,
    filters: ListOrdersInput,
    events: &BTreeSet<Event>,
) -> Result<(ListData, Warnings), ErrorCode> {
    let selected = reduce_orders(
        events,
        filters.currency(),
        Some(filters.status()),
        filters.side(),
    );
    ListData::from_selected(node_pubkey, filters, &selected)
}

fn failed(code: ErrorCode, warnings: Warnings, snapshot: Option<Snapshot>) -> ListOutcome {
    Envelope::error(Some(COMMAND), code, warnings, ExecutionMeta::read(snapshot))
}
