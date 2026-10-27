//! Public `order.list` data: normalized filters and selected order rows.
//!
//! Rows map explicit public fields only; no core or database record is
//! serialized wholesale. Monetary values and event times are decimal strings
//! so signed 64-bit values survive JSON consumers without precision loss.

use std::str::FromStr;

use chrono::{DateTime, Datelike, Utc};
use mostro_core::order::{Kind, SmallOrder, Status};
use nostr_sdk::prelude::{PublicKey, Timestamp};
use schemars::JsonSchema;
use serde::ser::Error as _;
use serde::{Serialize, Serializer};

use super::{schema, ErrorCode, WarningCode, Warnings};

/// Accepted `--side` values, matched case-insensitively.
pub const SIDES: [&str; 2] = ["buy", "sell"];

/// Accepted `--status` values: every pinned core status, matched
/// case-insensitively.
pub const STATUSES: [&str; 17] = [
    "active",
    "canceled",
    "canceled-by-admin",
    "settled-by-admin",
    "completed-by-admin",
    "dispute",
    "expired",
    "fiat-sent",
    "settled-hold-invoice",
    "pending",
    "success",
    "waiting-buyer-invoice",
    "waiting-payment",
    "waiting-taker-bond",
    "waiting-maker-bond",
    "cooperatively-canceled",
    "in-progress",
];

/// Normalized `order list` filters, serialized as `ListData.filters`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListOrdersInput {
    #[schemars(schema_with = "schema::nullable_side")]
    side: Option<Kind>,
    currency: Option<String>,
    #[schemars(schema_with = "schema::status")]
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
        let side = side
            .map(Kind::from_str)
            .transpose()
            .map_err(|()| ErrorCode::InvalidArguments)?;
        let status = status
            .map_or(Ok(Status::Pending), Status::from_str)
            .map_err(|()| ErrorCode::InvalidArguments)?;
        Ok(Self {
            side,
            currency: currency.map(str::to_uppercase),
            status,
        })
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PublicOrder {
    #[schemars(schema_with = "schema::uuid")]
    pub(crate) id: String,
    #[schemars(schema_with = "schema::side")]
    pub(crate) side: Kind,
    #[schemars(schema_with = "schema::status")]
    pub(crate) status: Status,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) sats_amount: String,
    pub(crate) fiat_currency: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) fiat_amount: String,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) fiat_amount_min: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) fiat_amount_max: Option<String>,
    pub(crate) payment_method: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) premium_percent: String,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) created_at: Option<String>,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) created_at_unix: String,
}

impl PublicOrder {
    fn from_selected(
        order: &SmallOrder,
        filters: &ListOrdersInput,
        warnings: &mut Warnings,
    ) -> Result<Self, ErrorCode> {
        let (Some(id), Some(side), Some(status), Some(created_at)) =
            (order.id, order.kind, order.status, order.created_at)
        else {
            return Err(ErrorCode::InternalError);
        };
        let matches = status == filters.status
            && filters.side.is_none_or(|wanted| wanted == side)
            && filters
                .currency
                .as_deref()
                .is_none_or(|wanted| wanted == order.fiat_code);
        if !matches {
            return Err(ErrorCode::InternalError);
        }
        let created_at_utc = rfc3339_utc(created_at);
        if created_at_utc.is_none() {
            warnings.add(WarningCode::InvalidEventTimestamp);
        }
        Ok(Self {
            id: id.to_string(),
            side,
            status,
            sats_amount: order.amount.to_string(),
            fiat_currency: order.fiat_code.clone(),
            fiat_amount: order.fiat_amount.to_string(),
            fiat_amount_min: order.min_amount.map(|amount| amount.to_string()),
            fiat_amount_max: order.max_amount.map(|amount| amount.to_string()),
            payment_method: order.payment_method.clone(),
            premium_percent: order.premium.to_string(),
            created_at: created_at_utc,
            created_at_unix: created_at.to_string(),
        })
    }
}

/// `order.list` success data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    pub(crate) node_pubkey: String,
    pub(crate) filters: ListOrdersInput,
    pub(crate) orders: Vec<PublicOrder>,
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
        let mut warnings = Warnings::new();
        let orders = selected
            .iter()
            .map(|order| PublicOrder::from_selected(order, &filters, &mut warnings))
            .collect::<Result<_, _>>()?;
        let data = Self {
            node_pubkey: node_pubkey.to_hex(),
            filters,
            orders,
        };
        Ok((data, warnings))
    }
}

/// UTC RFC3339 with `Z`, or `None` outside the four-digit years 0000-9999.
fn rfc3339_utc(seconds: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .filter(|at| (0..=9999).contains(&at.year()))
        .map(|at| at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompletionPolicy {
    AllRelaysEose,
}

/// Evidence of one bounded public read, serialized as `meta.snapshot`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Snapshot {
    #[serde(serialize_with = "utc_rfc3339")]
    #[schemars(schema_with = "schema::rfc3339")]
    since: Timestamp,
    /// No upper time filter is sent; always null.
    until: (),
    #[serde(serialize_with = "utc_rfc3339")]
    #[schemars(schema_with = "schema::rfc3339")]
    observed_at: Timestamp,
    #[schemars(schema_with = "schema::per_relay_limit")]
    per_relay_limit: usize,
    #[schemars(schema_with = "schema::max_unique_events")]
    max_unique_events: usize,
    #[schemars(schema_with = "schema::positive_count")]
    pub(crate) configured_relays: usize,
    #[schemars(schema_with = "schema::count")]
    pub(crate) completed_relays: usize,
    #[schemars(schema_with = "schema::received_events")]
    pub(crate) received_events: usize,
    completion_policy: CompletionPolicy,
    #[schemars(schema_with = "schema::never_complete_history")]
    complete_history: bool,
}

fn utc_rfc3339<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
    i64::try_from(at.as_secs())
        .ok()
        .and_then(rfc3339_utc)
        .ok_or_else(|| S::Error::custom("snapshot time outside four-digit years"))
        .and_then(|text| serializer.serialize_str(&text))
}

impl Snapshot {
    pub(crate) fn new(
        since: Timestamp,
        observed_at: Timestamp,
        per_relay_limit: usize,
        max_unique_events: usize,
        configured_relays: usize,
        completed_relays: usize,
        received_events: usize,
    ) -> Self {
        Self {
            since,
            until: (),
            observed_at,
            per_relay_limit,
            max_unique_events,
            configured_relays,
            completed_relays,
            received_events,
            completion_policy: CompletionPolicy::AllRelaysEose,
            complete_history: false,
        }
    }
}
