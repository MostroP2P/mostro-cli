//! Public `order.list` data: normalized filters and selected order rows.
//!
//! Rows map explicit public fields only; no core or database record is
//! serialized wholesale. Monetary values and event times are decimal strings
//! so signed 64-bit values survive JSON consumers without precision loss.

use mostro_core::order::{Kind, SmallOrder, Status};
use nostr_sdk::prelude::PublicKey;
use serde::Serialize;

use super::{ErrorCode, Warnings};

/// Normalized `order list` filters, serialized as `ListData.filters`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListOrdersInput {
    side: Option<Kind>,
    currency: Option<String>,
    status: Status,
}

impl ListOrdersInput {
    /// Parse raw filter values. Side and status are case-insensitive and not
    /// trimmed; status defaults to pending. Currency is uppercased as given,
    /// and an empty currency stays an exact filter rather than becoming null.
    pub fn parse(
        side: Option<&str>,
        currency: Option<&str>,
        status: Option<&str>,
    ) -> Result<Self, ErrorCode> {
        let _ = (side, currency, status);
        todo!("T022")
    }

    pub fn side(&self) -> Option<Kind> {
        self.side
    }

    pub fn currency(&self) -> Option<&str> {
        self.currency.as_deref()
    }

    pub fn status(&self) -> Status {
        self.status
    }
}

/// One selected order from the public book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicOrder {
    id: String,
    side: Kind,
    status: Status,
    sats_amount: String,
    fiat_currency: String,
    fiat_amount: String,
    fiat_amount_min: Option<String>,
    fiat_amount_max: Option<String>,
    payment_method: String,
    premium_percent: String,
    created_at: Option<String>,
    created_at_unix: String,
}

/// `order.list` success data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListData {
    node_pubkey: String,
    filters: ListOrdersInput,
    orders: Vec<PublicOrder>,
}

impl ListData {
    /// Map orders already reduced and filtered by `filters`, keeping their
    /// order. Event times outside four-digit UTC years add one aggregate
    /// `invalid_event_timestamp` warning. A row without id, side, status or
    /// event time, or one that does not match `filters`, is an
    /// `internal_error`.
    pub fn from_selected(
        node_pubkey: &PublicKey,
        filters: ListOrdersInput,
        selected: &[SmallOrder],
    ) -> Result<(Self, Warnings), ErrorCode> {
        let _ = (node_pubkey, filters, selected);
        todo!("T022")
    }
}
