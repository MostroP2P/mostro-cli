//! `order.info` data: the node's record of the user's own orders.
//!
//! The reply of the node is untrusted input. Only orders that were requested
//! are kept, each once, in the order of the request; every requested id is
//! either returned or listed as not returned. Records map explicit fields only:
//! no core object is serialized wholesale and the buyer invoice is never copied.

use std::collections::HashMap;
use std::str::FromStr;

use mostro_core::order::{Kind, SmallOrder, Status};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::order::rfc3339_utc;
use super::{schema, WarningCode, Warnings};

/// The distinct order ids of one request, in the order they were given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderInfoInput {
    ids: Vec<Uuid>,
}

impl OrderInfoInput {
    /// `None` when no id is given. A repeated id is kept once, at its first
    /// position. There is no upper limit: the node refuses what it will not answer.
    pub fn new(ids: impl IntoIterator<Item = Uuid>) -> Option<Self> {
        let mut distinct: Vec<Uuid> = Vec::new();
        for id in ids {
            if !distinct.contains(&id) {
                distinct.push(id);
            }
        }
        (!distinct.is_empty()).then_some(Self { ids: distinct })
    }

    pub fn ids(&self) -> &[Uuid] {
        &self.ids
    }
}

/// One of the user's orders as the node returned it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderRecord {
    #[schemars(schema_with = "schema::uuid")]
    id: String,
    #[schemars(schema_with = "schema::nullable_side")]
    side: Option<Kind>,
    #[schemars(schema_with = "schema::nullable_status")]
    status: Option<Status>,
    #[schemars(schema_with = "schema::decimal")]
    sats_amount: String,
    fiat_currency: String,
    #[schemars(schema_with = "schema::decimal")]
    fiat_amount: String,
    #[schemars(schema_with = "schema::nullable_decimal")]
    fiat_amount_min: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    fiat_amount_max: Option<String>,
    payment_method: String,
    #[schemars(schema_with = "schema::decimal")]
    premium_percent: String,
    #[schemars(schema_with = "schema::nullable_pubkey")]
    buyer_trade_pubkey: Option<String>,
    #[schemars(schema_with = "schema::nullable_pubkey")]
    seller_trade_pubkey: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    created_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    created_at_unix: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    expires_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    expires_at_unix: Option<String>,
}

/// `order.info` success data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderInfoData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    orders: Vec<OrderRecord>,
    #[schemars(schema_with = "schema::uuid_list")]
    not_returned: Vec<String>,
}

impl OrderInfoData {
    /// Map the node's `reply` for `requested`. An empty reply is the node's
    /// answer that none of the ids is the caller's: every id is not returned.
    /// One warning marks not-returned ids and one marks unrepresentable times.
    pub fn from_reply(
        node: &PublicKey,
        requested: &OrderInfoInput,
        reply: &[SmallOrder],
    ) -> (Self, Warnings) {
        let mut first: HashMap<Uuid, &SmallOrder> = HashMap::new();
        for order in reply {
            if let Some(id) = order.id {
                first.entry(id).or_insert(order);
            }
        }
        let mut warnings = Warnings::new();
        let mut orders = Vec::new();
        let mut not_returned = Vec::new();
        for id in requested.ids() {
            match first.get(id) {
                Some(order) => orders.push(OrderRecord::from_order(*id, order, &mut warnings)),
                None => not_returned.push(id.to_string()),
            }
        }
        if !not_returned.is_empty() {
            warnings.add(WarningCode::OrdersNotReturned);
        }
        let data = Self {
            node_pubkey: node.to_hex(),
            orders,
            not_returned,
        };
        (data, warnings)
    }

    pub fn orders(&self) -> &[OrderRecord] {
        &self.orders
    }

    pub fn not_returned(&self) -> &[String] {
        &self.not_returned
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }
}

impl OrderRecord {
    pub(super) fn from_order(id: Uuid, order: &SmallOrder, warnings: &mut Warnings) -> Self {
        let (created_at, created_at_unix) = time_fields(order.created_at, warnings);
        let (expires_at, expires_at_unix) = time_fields(order.expires_at, warnings);
        Self {
            id: id.to_string(),
            side: order.kind,
            status: order.status,
            sats_amount: order.amount.to_string(),
            fiat_currency: order.fiat_code.clone(),
            fiat_amount: order.fiat_amount.to_string(),
            fiat_amount_min: order.min_amount.map(|amount| amount.to_string()),
            fiat_amount_max: order.max_amount.map(|amount| amount.to_string()),
            payment_method: order.payment_method.clone(),
            premium_percent: order.premium.to_string(),
            buyer_trade_pubkey: trade_key(order.buyer_trade_pubkey.as_deref()),
            seller_trade_pubkey: trade_key(order.seller_trade_pubkey.as_deref()),
            created_at,
            created_at_unix,
            expires_at,
            expires_at_unix,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn side(&self) -> Option<Kind> {
        self.side
    }

    pub fn status(&self) -> Option<Status> {
        self.status
    }

    pub fn sats_amount(&self) -> &str {
        &self.sats_amount
    }

    pub fn fiat_currency(&self) -> &str {
        &self.fiat_currency
    }

    pub fn fiat_amount(&self) -> &str {
        &self.fiat_amount
    }

    pub fn fiat_amount_min(&self) -> Option<&str> {
        self.fiat_amount_min.as_deref()
    }

    pub fn fiat_amount_max(&self) -> Option<&str> {
        self.fiat_amount_max.as_deref()
    }

    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }

    pub fn premium_percent(&self) -> &str {
        &self.premium_percent
    }

    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    pub fn created_at_unix(&self) -> Option<&str> {
        self.created_at_unix.as_deref()
    }

    pub fn expires_at_unix(&self) -> Option<&str> {
        self.expires_at_unix.as_deref()
    }

    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }
}

/// UTC text and exact integer for an optional Unix time; the text is `None`
/// (with one warning) when the time is outside the four-digit years.
fn time_fields(seconds: Option<i64>, warnings: &mut Warnings) -> (Option<String>, Option<String>) {
    let Some(seconds) = seconds else {
        return (None, None);
    };
    let text = rfc3339_utc(seconds);
    if text.is_none() {
        warnings.add(WarningCode::InvalidOrderTimestamp);
    }
    (text, Some(seconds.to_string()))
}

/// A trade public key as lowercase hex, or `None` when the node sent something
/// that does not parse as a public key.
fn trade_key(value: Option<&str>) -> Option<String> {
    value
        .and_then(|text| PublicKey::from_str(text).ok())
        .map(|key| key.to_hex())
}
