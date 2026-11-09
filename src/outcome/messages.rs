//! `message.list` input and data: the messages the node sent to the user's
//! trade keys, as typed records.
//!
//! The node's messages are untrusted in the sense that matters for output: any
//! text in them is data, bounded here and escaped by the human renderer. A
//! record names the wire action and, for the few actions that ask the user for
//! something, a typed next step, so a caller can continue the flow without
//! reading prose. The invoices the node sends (bond, hold, payout request) are
//! data of the record and appear nowhere else.

use std::fmt;

use mostro_core::prelude::{Action, Message};
use nostr_sdk::prelude::{EventId, PublicKey};
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use uuid::Uuid;

mod payload;

pub use payload::MessagePayload;
pub(crate) use payload::{bounded, TEXT_BYTES};

use super::order::rfc3339_utc;
use super::{schema, ErrorCode, WarningCode, Warnings};
use payload::summarize;

pub const DEFAULT_SINCE_MINUTES: i64 = 30;
pub const MAX_SINCE_MINUTES: i64 = 10_080;
/// Trade keys asked about at most, the newest first.
pub const MAX_KEYS: usize = 200;
/// Records returned at most.
pub const MAX_MESSAGES: usize = 200;

/// Codes that fail `message.list` once it is resolved.
pub const ERRORS: [ErrorCode; 12] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::ReadTimeout,
    ErrorCode::ReadLimitExceeded,
    ErrorCode::InternalError,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
    ErrorCode::OrderUnknown,
    ErrorCode::StoreUnreadable,
];

/// Warnings of `message.list` beyond the pilot pair.
pub const WARNINGS: [WarningCode; 1] = [WarningCode::TradeKeysLimited];

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidMessageList;

impl fmt::Display for InvalidMessageList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid value for --since")
    }
}

impl std::error::Error for InvalidMessageList {}

/// A validated `message list`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageListInput {
    since_minutes: i64,
    order: Option<Uuid>,
}

impl MessageListInput {
    pub fn validate(since: Option<i64>, order: Option<Uuid>) -> Result<Self, InvalidMessageList> {
        let since_minutes = since.unwrap_or(DEFAULT_SINCE_MINUTES);
        if !(1..=MAX_SINCE_MINUTES).contains(&since_minutes) {
            return Err(InvalidMessageList);
        }
        Ok(Self {
            since_minutes,
            order,
        })
    }

    pub fn since_minutes(&self) -> i64 {
        self.since_minutes
    }

    pub fn order(&self) -> Option<Uuid> {
        self.order
    }
}

/// What the node asks the user to do next, for the actions that ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    /// Pay the bond invoice in the payload.
    PayBondInvoice,
    /// Send an invoice for the order: `order add-invoice`.
    AddInvoice,
    /// Pay the hold invoice in the payload.
    PayHoldInvoice,
    /// Send a payout invoice for a slashed bond: `bond add-invoice`.
    AddBondInvoice,
}

impl NextStep {
    pub fn of(action: &Action) -> Option<Self> {
        match action {
            Action::PayBondInvoice => Some(Self::PayBondInvoice),
            Action::AddInvoice => Some(Self::AddInvoice),
            Action::PayInvoice => Some(Self::PayHoldInvoice),
            Action::AddBondInvoice => Some(Self::AddBondInvoice),
            _ => None,
        }
    }
}

/// One message of the node to one of the user's trade keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MessageRecord {
    #[schemars(schema_with = "schema::pubkey_hex")]
    event_id: String,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    created_at: Option<String>,
    #[schemars(schema_with = "schema::decimal")]
    created_at_unix: String,
    #[schemars(schema_with = "schema::nullable_uuid")]
    order_id: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    request_id: Option<String>,
    /// The wire name of the action.
    #[schemars(schema_with = "action_schema")]
    action: String,
    next_step: Option<NextStep>,
    /// The index of the key the message was addressed to; null when unknown.
    #[schemars(schema_with = "schema::nullable_decimal")]
    trade_index: Option<String>,
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
    payload: MessagePayload,
}

fn action_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": "^[a-z][a-z0-9-]{0,63}$"})
}

impl MessageRecord {
    /// Summarize `message`, which was addressed to `trade` (index `trade_index`
    /// when known) and published at `created_at` seconds as event `event`.
    /// A time outside four-digit UTC years adds the aggregate
    /// `invalid_event_timestamp` warning and leaves the RFC 3339 field null.
    pub fn from_message(
        event: &EventId,
        created_at: u64,
        message: &Message,
        trade_index: Option<i64>,
        trade: &PublicKey,
        warnings: &mut Warnings,
    ) -> Self {
        let kind = message.get_inner_message_kind();
        let unix = i64::try_from(created_at).unwrap_or(i64::MAX);
        let created = rfc3339_utc(unix);
        if created.is_none() {
            warnings.add(WarningCode::InvalidEventTimestamp);
        }
        Self {
            event_id: event.to_hex(),
            created_at: created,
            created_at_unix: unix.to_string(),
            order_id: kind.id.map(|id| id.to_string()),
            request_id: kind.request_id.map(|id| id.to_string()),
            action: wire_action(&kind.action),
            next_step: NextStep::of(&kind.action),
            trade_index: trade_index.map(|index| index.to_string()),
            trade_pubkey: trade.to_hex(),
            payload: summarize(kind.id, kind.payload.as_ref(), warnings),
        }
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn created_at_unix(&self) -> &str {
        &self.created_at_unix
    }

    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    pub fn order_id(&self) -> Option<&str> {
        self.order_id.as_deref()
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn next_step(&self) -> Option<NextStep> {
        self.next_step
    }

    pub fn trade_index(&self) -> Option<&str> {
        self.trade_index.as_deref()
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }

    pub fn payload(&self) -> &MessagePayload {
        &self.payload
    }
}

/// The wire name of an action, restricted to what the schema allows.
fn wire_action(action: &Action) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 64
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// `message.list` success data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MessageListData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::decimal")]
    since_minutes: String,
    /// Oldest first.
    messages: Vec<MessageRecord>,
    /// The result may not be everything: a relay reached its per-relay limit or
    /// more than the record cap matched.
    truncated: bool,
    #[schemars(schema_with = "schema::decimal")]
    keys_read: String,
    #[schemars(schema_with = "schema::decimal")]
    keys_not_read: String,
}

impl MessageListData {
    pub fn new(
        node: &PublicKey,
        input: &MessageListInput,
        messages: Vec<MessageRecord>,
        truncated: bool,
        (keys_read, keys_not_read): (usize, usize),
    ) -> Self {
        Self {
            node_pubkey: node.to_hex(),
            since_minutes: input.since_minutes.to_string(),
            messages,
            truncated,
            keys_read: keys_read.to_string(),
            keys_not_read: keys_not_read.to_string(),
        }
    }

    pub fn messages(&self) -> &[MessageRecord] {
        &self.messages
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn since_minutes(&self) -> &str {
        &self.since_minutes
    }

    pub fn keys_read(&self) -> &str {
        &self.keys_read
    }

    pub fn keys_not_read(&self) -> &str {
        &self.keys_not_read
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::order::Snapshot;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::schema::{output_schema, OutcomeRules};
    use crate::outcome::{Envelope, ExecutionMeta};
    use mostro_core::order::{Kind, SmallOrder, Status};
    use mostro_core::prelude::{BondPayoutRequest, CantDoReason, Payload, Peer};
    use nostr_sdk::prelude::{Keys, Timestamp};
    use serde_json::{json, Value};

    const AT: u64 = 1_792_108_800;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn order(n: u128, status: Status) -> SmallOrder {
        SmallOrder::new(
            Some(id(n)),
            Some(Kind::Sell),
            Some(status),
            6_233,
            "USD".to_string(),
            None,
            None,
            5,
            "bank".to_string(),
            1,
            None,
            None,
            None,
            Some(1_792_108_800),
            Some(1_792_195_200),
        )
    }

    fn message(action: Action, order_id: Option<Uuid>, payload: Option<Payload>) -> Message {
        Message::new_order(order_id, Some(77), None, action, payload)
    }

    fn record(message: &Message) -> (MessageRecord, Warnings) {
        let mut warnings = Warnings::new();
        let event = EventId::from_byte_array([7; 32]);
        let trade = Keys::generate().public_key();
        let record =
            MessageRecord::from_message(&event, AT, message, Some(3), &trade, &mut warnings);
        (record, warnings)
    }

    #[test]
    fn the_window_defaults_to_thirty_minutes_and_is_bounded() {
        let input = MessageListInput::validate(None, None).unwrap();
        assert_eq!((input.since_minutes(), input.order()), (30, None));
        for good in [1, 30, 10_080] {
            assert!(
                MessageListInput::validate(Some(good), None).is_ok(),
                "{good}"
            );
        }
        for bad in [0, -1, -5, 10_081, i64::MIN, i64::MAX] {
            let error = MessageListInput::validate(Some(bad), None).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --since", "{bad}");
        }
        let kept = MessageListInput::validate(Some(5), Some(id(4))).unwrap();
        assert_eq!(kept.order(), Some(id(4)));
    }

    #[test]
    fn only_the_actions_that_ask_for_something_have_a_next_step() {
        assert_eq!(
            NextStep::of(&Action::PayBondInvoice),
            Some(NextStep::PayBondInvoice)
        );
        assert_eq!(
            NextStep::of(&Action::AddInvoice),
            Some(NextStep::AddInvoice)
        );
        assert_eq!(
            NextStep::of(&Action::PayInvoice),
            Some(NextStep::PayHoldInvoice)
        );
        assert_eq!(
            NextStep::of(&Action::AddBondInvoice),
            Some(NextStep::AddBondInvoice)
        );
        for quiet in [
            Action::WaitingSellerToPay,
            Action::CantDo,
            Action::HoldInvoicePaymentAccepted,
            Action::BondInvoiceAccepted,
            Action::NewOrder,
            Action::Released,
        ] {
            assert_eq!(NextStep::of(&quiet), None, "{quiet:?}");
        }
    }

    #[test]
    fn a_bond_request_keeps_the_invoice_the_order_and_the_next_step() {
        let parked = order(1, Status::Pending);
        let (record, warnings) = record(&message(
            Action::PayBondInvoice,
            Some(id(1)),
            Some(Payload::PaymentRequest(
                Some(parked),
                "lnbcrt1bond".to_string(),
                None,
            )),
        ));

        assert_eq!(record.action(), "pay-bond-invoice");
        assert_eq!(record.next_step(), Some(NextStep::PayBondInvoice));
        assert_eq!(record.order_id(), Some(id(1).to_string().as_str()));
        assert_eq!(record.request_id(), Some("77"));
        assert_eq!(record.trade_index(), Some("3"));
        assert_eq!(record.created_at_unix(), AT.to_string());
        assert_eq!(record.created_at(), Some("2026-10-16T00:00:00Z"));
        match record.payload() {
            MessagePayload::PaymentRequest {
                order,
                invoice,
                amount,
            } => {
                assert_eq!(invoice, "lnbcrt1bond");
                assert_eq!(order.as_ref().unwrap().sats_amount(), "6233");
                assert_eq!(amount, &None);
            }
            other => panic!("{other:?}"),
        }
        assert!(warnings.is_empty());
    }

    #[test]
    fn each_payload_has_its_summary() {
        let cases: Vec<(Message, Value)> = vec![
            (
                message(Action::WaitingSellerToPay, Some(id(1)), None),
                json!({"kind": "none"}),
            ),
            (
                message(
                    Action::CantDo,
                    None,
                    Some(Payload::CantDo(Some(CantDoReason::InvalidInvoice))),
                ),
                json!({"kind": "cant_do", "reason": "invalid_invoice"}),
            ),
            (
                message(Action::CantDo, None, Some(Payload::CantDo(None))),
                json!({"kind": "cant_do", "reason": null}),
            ),
            (
                message(
                    Action::HoldInvoicePaymentAccepted,
                    Some(id(1)),
                    Some(Payload::TextMessage("hi".into())),
                ),
                json!({"kind": "text", "text": "hi"}),
            ),
            (
                message(Action::TakeSell, Some(id(1)), Some(Payload::Amount(30))),
                json!({"kind": "amount", "amount": "30"}),
            ),
            (
                message(
                    Action::FiatSentOk,
                    Some(id(1)),
                    Some(Payload::Peer(Peer::new("ab".repeat(32), None))),
                ),
                json!({"kind": "peer", "counterparty_pubkey": "ab".repeat(32)}),
            ),
            (
                message(Action::RateUser, Some(id(1)), Some(Payload::RatingUser(5))),
                json!({"kind": "other", "variant": "rating_user"}),
            ),
            (
                message(Action::Orders, None, Some(Payload::Ids(vec![id(1)]))),
                json!({"kind": "other", "variant": "ids"}),
            ),
        ];
        for (message, expected) in cases {
            let (record, _) = record(&message);
            assert_eq!(serde_json::to_value(record.payload()).unwrap(), expected);
        }
    }

    #[test]
    fn a_bond_payout_request_carries_the_order_and_when_it_was_slashed() {
        let (record, _) = record(&message(
            Action::AddBondInvoice,
            Some(id(3)),
            Some(Payload::BondPayoutRequest(BondPayoutRequest {
                order: order(3, Status::Success),
                slashed_at: AT as i64,
            })),
        ));

        assert_eq!(record.next_step(), Some(NextStep::AddBondInvoice));
        match record.payload() {
            MessagePayload::BondPayoutRequest {
                order,
                slashed_at,
                slashed_at_unix,
            } => {
                assert_eq!(order.id(), id(3).to_string());
                assert_eq!(slashed_at.as_deref(), Some("2026-10-16T00:00:00Z"));
                assert_eq!(slashed_at_unix, &AT.to_string());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_order_payload_takes_its_id_from_the_message_when_it_has_none() {
        let mut bare = order(1, Status::WaitingBuyerInvoice);
        bare.id = None;
        let (named, _) = record(&message(
            Action::AddInvoice,
            Some(id(8)),
            Some(Payload::Order(bare.clone())),
        ));
        match named.payload() {
            MessagePayload::Order { order } => assert_eq!(order.id(), id(8).to_string()),
            other => panic!("{other:?}"),
        }
        let (anonymous, _) = record(&message(
            Action::AddInvoice,
            None,
            Some(Payload::Order(bare)),
        ));
        assert_eq!(
            serde_json::to_value(anonymous.payload()).unwrap(),
            json!({"kind": "other", "variant": "order"}),
            "an order with no id anywhere cannot be shown as an order"
        );
    }

    #[test]
    fn a_payment_request_keeps_an_invoice_longer_than_the_text_bound() {
        let invoice = format!("lnbcrt1{}", "q".repeat(TEXT_BYTES));
        let (record, _) = record(&message(
            Action::PayInvoice,
            Some(id(1)),
            Some(Payload::PaymentRequest(None, invoice.clone(), None)),
        ));
        match record.payload() {
            MessagePayload::PaymentRequest { invoice: kept, .. } => assert_eq!(kept, &invoice),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn text_from_the_node_is_bounded_on_a_character_boundary() {
        let long = "é".repeat(TEXT_BYTES);
        let (record, _) = record(&message(
            Action::HoldInvoicePaymentAccepted,
            Some(id(1)),
            Some(Payload::TextMessage(long)),
        ));
        match record.payload() {
            MessagePayload::Text { text } => {
                assert!(text.len() <= TEXT_BYTES && !text.is_empty());
                assert!(text.chars().all(|c| c == 'é'), "no split character");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_time_outside_four_digit_years_is_a_warning_and_a_null_field() {
        let mut warnings = Warnings::new();
        let record = MessageRecord::from_message(
            &EventId::from_byte_array([7; 32]),
            u64::MAX / 2,
            &message(Action::WaitingSellerToPay, Some(id(1)), None),
            None,
            &Keys::generate().public_key(),
            &mut warnings,
        );
        assert_eq!(record.created_at(), None);
        assert_eq!(record.trade_index(), None, "an unknown index is null");
        assert!(!warnings.is_empty());
    }

    fn snapshot() -> Snapshot {
        Snapshot::new(
            Timestamp::from(1_791_504_000),
            Timestamp::from(1_792_108_800),
            50,
            10_000,
            2,
            2,
            3,
        )
    }

    fn schema() -> Value {
        Value::Object(output_schema::<MessageListData, Snapshot>(&OutcomeRules {
            warnings: &WARNINGS,
            ..OutcomeRules::read("message.list", &ERRORS, true)
        }))
    }

    fn success(messages: Vec<MessageRecord>) -> Value {
        let node = Keys::generate().public_key();
        let input = MessageListInput::validate(Some(60), None).unwrap();
        let data = MessageListData::new(&node, &input, messages, false, (4, 0));
        serde_json::to_value(Envelope::ok(
            "message.list",
            data,
            Warnings::new(),
            ExecutionMeta::read(Some(snapshot())),
        ))
        .unwrap()
    }

    fn every_kind() -> Vec<MessageRecord> {
        let parked = order(1, Status::Pending);
        [
            message(
                Action::PayBondInvoice,
                Some(id(1)),
                Some(Payload::PaymentRequest(
                    Some(parked.clone()),
                    "lnbcrt1bond".into(),
                    None,
                )),
            ),
            message(
                Action::AddInvoice,
                Some(id(1)),
                Some(Payload::Order(parked.clone())),
            ),
            message(Action::WaitingSellerToPay, Some(id(1)), None),
            message(
                Action::CantDo,
                None,
                Some(Payload::CantDo(Some(CantDoReason::InvalidPeer))),
            ),
            message(
                Action::AddBondInvoice,
                Some(id(1)),
                Some(Payload::BondPayoutRequest(BondPayoutRequest {
                    order: parked,
                    slashed_at: AT as i64,
                })),
            ),
            message(
                Action::HoldInvoicePaymentAccepted,
                Some(id(1)),
                Some(Payload::TextMessage("hi".into())),
            ),
            message(Action::RateUser, Some(id(1)), Some(Payload::RatingUser(4))),
        ]
        .iter()
        .map(|message| record(message).0)
        .collect()
    }

    #[test]
    fn real_outcomes_conform_to_the_generated_schema() {
        let schema = schema();
        assert_eq!(schema["title"], "message.list outcome");
        for outcome in [success(Vec::new()), success(every_kind())] {
            assert!(conforms(&schema, &schema, &outcome), "{outcome}");
        }
        let codes: Vec<&str> = schema["$defs"]["ErrorCode"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for code in [
            "store_unreadable",
            "order_unknown",
            "identity_missing",
            "read_timeout",
        ] {
            assert!(codes.contains(&code), "{code}");
        }
        assert!(!codes.contains(&"store_unwritable"));
        assert!(
            !codes.contains(&"reply_timeout"),
            "a read has no uncertain status"
        );
    }

    #[test]
    fn a_read_has_no_uncertain_status() {
        let schema = schema();
        let mut outcome = success(Vec::new());
        outcome["status"] = json!("uncertain");
        assert!(!conforms(&schema, &schema, &outcome));
    }

    #[test]
    fn contradictory_outcomes_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let edits: Vec<(&str, Edit)> = vec![
            (
                "a bad event id",
                Box::new(|o| o["data"]["messages"][0]["event_id"] = json!("xyz")),
            ),
            (
                "an uppercase trade key",
                Box::new(|o| o["data"]["messages"][0]["trade_pubkey"] = json!("ABC")),
            ),
            (
                "a numeric trade index",
                Box::new(|o| o["data"]["messages"][0]["trade_index"] = json!(3)),
            ),
            (
                "a bad order id",
                Box::new(|o| o["data"]["messages"][0]["order_id"] = json!("nope")),
            ),
            (
                "an unknown next step",
                Box::new(|o| o["data"]["messages"][0]["next_step"] = json!("pay_now")),
            ),
            (
                "an uppercase action",
                Box::new(|o| o["data"]["messages"][0]["action"] = json!("Add-Invoice")),
            ),
            (
                "an unknown payload kind",
                Box::new(|o| o["data"]["messages"][0]["payload"] = json!({"kind": "raw"})),
            ),
            (
                "a numeric key count",
                Box::new(|o| o["data"]["keys_read"] = json!(4)),
            ),
            (
                "a missing truncated flag",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("truncated");
                }),
            ),
            (
                "a write stage",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "no snapshot",
                Box::new(|o| o["meta"]["snapshot"] = Value::Null),
            ),
            (
                "another command",
                Box::new(|o| o["command"] = json!("order.list")),
            ),
        ];
        let schema = schema();
        for (label, edit) in edits {
            let mut outcome = success(every_kind());
            edit(&mut outcome);
            assert!(!conforms(&schema, &schema, &outcome), "accepted: {label}");
        }
    }
}
