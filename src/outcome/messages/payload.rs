//! The summary of a message payload: a typed record for the variants the flow
//! needs, the variant name only for the rest. Text from the node is bounded
//! here and escaped by the human renderer.

use mostro_core::order::SmallOrder;
use mostro_core::prelude::{CantDoReason, Payload};
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use uuid::Uuid;

use crate::outcome::info::OrderRecord;
use crate::outcome::order::rfc3339_utc;
use crate::outcome::{schema, Warnings};

pub(super) const TEXT_BYTES: usize = 4096;
const PUBKEY_TEXT_BYTES: usize = 128;

/// A summary of the payload of a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePayload {
    /// The message carries none.
    None,
    Order {
        order: OrderRecord,
    },
    PaymentRequest {
        /// The order the invoice is for, when the node sent it.
        order: Option<OrderRecord>,
        /// The invoice to pay (or the one the node holds); data for the caller.
        invoice: String,
        #[schemars(schema_with = "schema::nullable_decimal")]
        amount: Option<String>,
    },
    BondPayoutRequest {
        order: OrderRecord,
        /// When the node recorded the slash.
        #[schemars(schema_with = "schema::nullable_rfc3339")]
        slashed_at: Option<String>,
        #[schemars(schema_with = "schema::decimal")]
        slashed_at_unix: String,
    },
    CantDo {
        #[schemars(schema_with = "reason_schema")]
        reason: Option<String>,
    },
    Peer {
        counterparty_pubkey: String,
    },
    Text {
        text: String,
    },
    Amount {
        #[schemars(schema_with = "schema::decimal")]
        amount: String,
    },
    /// Any other payload, by variant name only.
    Other {
        variant: String,
    },
}

fn reason_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "pattern": schema::REASON_PATTERN})
}

fn other(payload: &Payload) -> MessagePayload {
    let variant = serde_json::to_value(payload)
        .ok()
        .and_then(|value| value.as_object()?.keys().next().cloned())
        .filter(|name| {
            name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .unwrap_or_else(|| "unknown".to_string());
    MessagePayload::Other { variant }
}

/// At most `limit` bytes of `text`, cut on a character boundary.
fn bounded(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub(super) fn summarize(
    message_order: Option<Uuid>,
    payload: Option<&Payload>,
    warnings: &mut Warnings,
) -> MessagePayload {
    let Some(payload) = payload else {
        return MessagePayload::None;
    };
    let record = |order: &SmallOrder, warnings: &mut Warnings| {
        order
            .id
            .or(message_order)
            .map(|id| OrderRecord::from_order(id, order, warnings))
    };
    match payload {
        Payload::Order(order) => match record(order, warnings) {
            Some(order) => MessagePayload::Order { order },
            None => other(payload),
        },
        Payload::PaymentRequest(order, invoice, amount) => MessagePayload::PaymentRequest {
            order: order.as_ref().and_then(|order| record(order, warnings)),
            invoice: bounded(invoice, TEXT_BYTES),
            amount: amount.map(|amount| amount.to_string()),
        },
        Payload::BondPayoutRequest(request) => match record(&request.order, warnings) {
            Some(order) => MessagePayload::BondPayoutRequest {
                order,
                slashed_at: rfc3339_utc(request.slashed_at),
                slashed_at_unix: request.slashed_at.to_string(),
            },
            None => other(payload),
        },
        Payload::CantDo(reason) => MessagePayload::CantDo {
            reason: reason.as_ref().map(reason_word),
        },
        Payload::Peer(peer) if peer.pubkey.len() <= PUBKEY_TEXT_BYTES => MessagePayload::Peer {
            counterparty_pubkey: peer.pubkey.clone(),
        },
        Payload::TextMessage(text) => MessagePayload::Text {
            text: bounded(text, TEXT_BYTES),
        },
        Payload::Amount(amount) => MessagePayload::Amount {
            amount: amount.to_string(),
        },
        _ => other(payload),
    }
}

/// The refusal reason as its wire word, `unknown` when it is not one.
fn reason_word(reason: &CantDoReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|word| {
            word.starts_with(|c: char| c.is_ascii_lowercase())
                && word.len() <= 64
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .unwrap_or_else(|| "unknown".to_string())
}
