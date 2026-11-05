//! `order.list`: read the public order book.

use std::collections::BTreeSet;

use nostr_sdk::prelude::{Event, PublicKey};

use crate::outcome::order::{ListData, ListOrdersInput};
use crate::outcome::{ErrorCode, Warnings};

/// Reduce collected order events to their latest state per order id, apply
/// `filters`, and map the rows, newest event first.
pub fn select_orders(
    node_pubkey: &PublicKey,
    filters: ListOrdersInput,
    events: BTreeSet<Event>,
) -> Result<(ListData, Warnings), ErrorCode> {
    let _ = (node_pubkey, filters, events);
    todo!("T028")
}
