//! `order.take-sell` and `order.take-buy` input and data: the order the user
//! takes and what the node asked them to do next.
//!
//! Validation catches only what is cheap and certain; the node stays the
//! authority (the order's state and window, the range, the invoice amounts). A
//! failed check names the option, never its value. The node's reply is
//! untrusted: it must name the order that was taken and maps to explicit fields
//! only. The hold invoice is data for the caller; it appears nowhere else.

use std::fmt;

use mostro_core::order::{Kind, SmallOrder, Status};
use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::info::OrderRecord;
use super::{schema, Warnings};

const INVOICE_BYTES: usize = 4096;

/// Which order is taken. The side names the order, as everywhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeSide {
    /// A sell order: the taker buys sats (`order.take-sell`).
    Sell,
    /// A buy order: the taker sells sats (`order.take-buy`).
    Buy,
}

impl TakeSide {
    pub fn command(self) -> &'static str {
        match self {
            Self::Sell => "order.take-sell",
            Self::Buy => "order.take-buy",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Self::Sell => Action::TakeSell,
            Self::Buy => Action::TakeBuy,
        }
    }

    pub fn order_kind(self) -> Kind {
        match self {
            Self::Sell => Kind::Sell,
            Self::Buy => Kind::Buy,
        }
    }
}

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidTake {
    Amount,
    Invoice,
}

impl InvalidTake {
    pub fn option(self) -> &'static str {
        match self {
            Self::Amount => "--amount",
            Self::Invoice => "--invoice",
        }
    }
}

impl fmt::Display for InvalidTake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid value for {}", self.option())
    }
}

impl std::error::Error for InvalidTake {}

/// What the user typed, before validation.
#[derive(Clone, Debug)]
pub struct TakeRequest {
    pub side: TakeSide,
    pub order: Uuid,
    pub amount: Option<i64>,
    pub invoice: Option<String>,
}

/// A validated take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderTakeInput {
    side: TakeSide,
    order: Uuid,
    amount: Option<i64>,
    invoice: Option<String>,
}

impl OrderTakeInput {
    pub fn validate(request: TakeRequest) -> Result<Self, InvalidTake> {
        if request.amount.is_some_and(|amount| amount <= 0) {
            return Err(InvalidTake::Amount);
        }
        let invoice = request.invoice.map(|text| text.trim().to_string());
        if invoice.as_ref().is_some_and(|text| {
            request.side == TakeSide::Buy
                || text.is_empty()
                || text.len() > INVOICE_BYTES
                || text.chars().any(|c| c.is_control() || c.is_whitespace())
        }) {
            return Err(InvalidTake::Invoice);
        }
        Ok(Self {
            side: request.side,
            order: request.order,
            amount: request.amount,
            invoice,
        })
    }

    /// The payload of the take, byte for byte what the legacy commands sent:
    /// a take-buy carries an amount when one is given; a take-sell carries the
    /// invoice with the amount inside the same request, or an amount, `0` when
    /// there is neither.
    pub fn payload(&self) -> Option<Payload> {
        match (self.side, &self.invoice) {
            (TakeSide::Buy, _) => self.amount.map(Payload::Amount),
            (TakeSide::Sell, Some(invoice)) => {
                Some(Payload::PaymentRequest(None, invoice.clone(), self.amount))
            }
            (TakeSide::Sell, None) => Some(Payload::Amount(self.amount.unwrap_or(0))),
        }
    }

    /// The row to keep when the node's reply carries no order: the id, the side
    /// of the order, the status the take put it in and the amount taken. What
    /// the node did not send stays empty.
    pub fn minimal_order(&self) -> SmallOrder {
        SmallOrder::new(
            Some(self.order),
            Some(self.side.order_kind()),
            Some(Status::WaitingPayment),
            0,
            String::new(),
            None,
            None,
            self.amount.unwrap_or(0),
            String::new(),
            0,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub fn side(&self) -> TakeSide {
        self.side
    }

    pub fn order(&self) -> Uuid {
        self.order
    }

    pub fn amount(&self) -> Option<i64> {
        self.amount
    }

    pub fn has_invoice(&self) -> bool {
        self.invoice.is_some()
    }
}

/// What the node asked the taker to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    /// Send a buyer invoice for the order.
    AddInvoice,
    /// The invoice was accepted; the seller has to pay.
    WaitForSellerPayment,
    /// Pay the hold invoice in `payment_request`.
    PayHoldInvoice,
}

/// A reply of the node to a take, with the payload it must carry.
#[derive(Clone, Debug)]
pub enum Taken {
    /// `add-invoice` with the order.
    AddInvoice(SmallOrder),
    /// `waiting-seller-to-pay`: no payload.
    WaitingSellerToPay,
    /// `pay-invoice` with the order and the hold invoice.
    PayInvoice(SmallOrder, String),
}

/// `order.take-sell` / `order.take-buy` success data (and the data of an
/// `order_not_saved` outcome).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderTakeData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    #[schemars(schema_with = "schema::side")]
    side: Kind,
    next_step: NextStep,
    /// The node's order; null when the reply carried none.
    order: Option<OrderRecord>,
    /// The hold invoice to pay; present only with `pay_hold_invoice`.
    payment_request: Option<String>,
    #[schemars(schema_with = "schema::decimal")]
    trade_index: String,
    /// The public key of the take's trade key; the private key stays derivable
    /// from the mnemonic and the index and is never shown.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl OrderTakeData {
    /// Map the node's `taken` reply for `input`. Returns the data, the order to
    /// save locally (the node's, or the minimal one) and the warnings; `None`
    /// when the reply names another order or carries no id, so it cannot be used.
    pub fn from_taken(
        node: &PublicKey,
        input: &OrderTakeInput,
        taken: &Taken,
        trade_index: i64,
        trade_pubkey: &PublicKey,
    ) -> Option<(Self, SmallOrder, Warnings)> {
        let (next_step, node_order, payment_request) = match taken {
            Taken::AddInvoice(order) => (NextStep::AddInvoice, Some(order), None),
            Taken::WaitingSellerToPay => (NextStep::WaitForSellerPayment, None, None),
            Taken::PayInvoice(order, invoice) => {
                (NextStep::PayHoldInvoice, Some(order), Some(invoice.clone()))
            }
        };
        if node_order.is_some_and(|order| order.id != Some(input.order)) {
            return None;
        }
        let mut warnings = Warnings::new();
        let record =
            node_order.map(|order| OrderRecord::from_order(input.order, order, &mut warnings));
        let to_save = node_order.cloned().unwrap_or_else(|| input.minimal_order());
        let data = Self {
            node_pubkey: node.to_hex(),
            order_id: input.order.to_string(),
            side: input.side.order_kind(),
            next_step,
            order: record,
            payment_request,
            trade_index: trade_index.to_string(),
            trade_pubkey: trade_pubkey.to_hex(),
        };
        Some((data, to_save, warnings))
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn side(&self) -> Kind {
        self.side
    }

    pub fn next_step(&self) -> NextStep {
        self.next_step
    }

    pub fn order(&self) -> Option<&OrderRecord> {
        self.order.as_ref()
    }

    pub fn payment_request(&self) -> Option<&str> {
        self.payment_request.as_deref()
    }

    pub fn trade_index(&self) -> &str {
        &self.trade_index
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::schema::{output_schema, OutcomeRules};
    use crate::outcome::{
        DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication,
        Status as OutcomeStatus, WarningCode,
    };
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};

    fn order_id() -> Uuid {
        Uuid::from_u128(9)
    }

    fn request(side: TakeSide) -> TakeRequest {
        TakeRequest {
            side,
            order: order_id(),
            amount: None,
            invoice: None,
        }
    }

    fn input(side: TakeSide, edit: impl FnOnce(&mut TakeRequest)) -> OrderTakeInput {
        let mut raw = request(side);
        edit(&mut raw);
        OrderTakeInput::validate(raw).unwrap()
    }

    fn wire(input: &OrderTakeInput) -> Value {
        serde_json::to_value(input.payload()).unwrap()
    }

    #[test]
    fn the_sides_name_their_command_action_and_order_kind() {
        assert_eq!(TakeSide::Sell.command(), "order.take-sell");
        assert_eq!(TakeSide::Buy.command(), "order.take-buy");
        assert_eq!(TakeSide::Sell.action(), Action::TakeSell);
        assert_eq!(TakeSide::Buy.action(), Action::TakeBuy);
        assert_eq!(TakeSide::Sell.order_kind(), Kind::Sell);
        assert_eq!(TakeSide::Buy.order_kind(), Kind::Buy);
    }

    #[test]
    fn the_payload_is_what_the_legacy_commands_sent() {
        // take-buy: an amount only when given, and nothing else.
        assert_eq!(wire(&input(TakeSide::Buy, |_| {})), Value::Null);
        assert_eq!(
            wire(&input(TakeSide::Buy, |r| r.amount = Some(150))),
            json!({"amount": 150})
        );
        // take-sell without an invoice: an amount, zero when none is given.
        assert_eq!(wire(&input(TakeSide::Sell, |_| {})), json!({"amount": 0}));
        assert_eq!(
            wire(&input(TakeSide::Sell, |r| r.amount = Some(150))),
            json!({"amount": 150})
        );
        // take-sell with an invoice: the amount rides inside the request.
        let address = "buyer@example.com";
        assert_eq!(
            wire(&input(TakeSide::Sell, |r| r.invoice = Some(address.into()))),
            json!({"payment_request": [null, address, null]})
        );
        assert_eq!(
            wire(&input(TakeSide::Sell, |r| {
                r.invoice = Some(format!(" {address} "));
                r.amount = Some(150);
            })),
            json!({"payment_request": [null, address, 150]})
        );
    }

    #[test]
    fn each_rule_names_only_its_option_and_an_invalid_invoice_is_no_longer_sent() {
        let bad = |side, edit: &dyn Fn(&mut TakeRequest)| {
            let mut raw = request(side);
            edit(&mut raw);
            OrderTakeInput::validate(raw).unwrap_err()
        };
        assert_eq!(
            bad(TakeSide::Sell, &|r| r.amount = Some(0)),
            InvalidTake::Amount
        );
        assert_eq!(
            bad(TakeSide::Buy, &|r| r.amount = Some(-3)),
            InvalidTake::Amount
        );
        assert_eq!(
            bad(TakeSide::Sell, &|r| r.invoice = Some("".into())),
            InvalidTake::Invoice
        );
        assert_eq!(
            bad(TakeSide::Sell, &|r| r.invoice =
                Some("not an invoice".into())),
            InvalidTake::Invoice
        );
        assert_eq!(
            bad(TakeSide::Sell, &|r| r.invoice = Some("ln\u{1b}[2J".into())),
            InvalidTake::Invoice
        );
        assert_eq!(
            bad(TakeSide::Sell, &|r| r.invoice = Some("x".repeat(4097))),
            InvalidTake::Invoice
        );
        assert_eq!(
            bad(TakeSide::Buy, &|r| r.invoice = Some("lnbc1".into())),
            InvalidTake::Invoice,
            "a take-buy has no invoice"
        );
        let error = bad(TakeSide::Sell, &|r| {
            r.invoice = Some("SENTINEL value".into())
        });
        assert_eq!(error.to_string(), "invalid value for --invoice");
        assert!(!error.to_string().contains("SENTINEL"));
    }

    fn node_order(status: Status) -> SmallOrder {
        SmallOrder::new(
            Some(order_id()),
            Some(Kind::Sell),
            Some(status),
            15_000,
            "USD".to_string(),
            None,
            None,
            15,
            "bank transfer".to_string(),
            1,
            None,
            None,
            None,
            Some(1_792_108_800),
            Some(1_792_195_200),
        )
    }

    fn keys() -> (PublicKey, PublicKey) {
        (Keys::generate().public_key(), Keys::generate().public_key())
    }

    fn map(side: TakeSide, taken: &Taken) -> Option<(OrderTakeData, SmallOrder, Warnings)> {
        let (node, trade) = keys();
        OrderTakeData::from_taken(&node, &input(side, |_| {}), taken, 4, &trade)
    }

    #[test]
    fn each_reply_maps_to_its_next_step() {
        let (data, saved, warnings) = map(
            TakeSide::Sell,
            &Taken::AddInvoice(node_order(Status::WaitingBuyerInvoice)),
        )
        .unwrap();
        assert_eq!(data.next_step(), NextStep::AddInvoice);
        assert_eq!(data.order().unwrap().id(), order_id().to_string());
        assert_eq!((data.payment_request(), data.trade_index()), (None, "4"));
        assert_eq!(saved.status, Some(Status::WaitingBuyerInvoice));
        assert!(warnings.is_empty());

        let (data, saved, _) = map(TakeSide::Sell, &Taken::WaitingSellerToPay).unwrap();
        assert_eq!(data.next_step(), NextStep::WaitForSellerPayment);
        assert!(data.order().is_none() && data.payment_request().is_none());
        assert_eq!(data.side(), Kind::Sell);
        assert_eq!(saved.id, Some(order_id()), "the minimal row");
        assert_eq!(saved.status, Some(Status::WaitingPayment));
        assert_eq!(
            (saved.fiat_code.as_str(), saved.payment_method.as_str()),
            ("", "")
        );

        let (data, saved, _) = map(
            TakeSide::Buy,
            &Taken::PayInvoice(node_order(Status::WaitingPayment), "lnbc1hold".into()),
        )
        .unwrap();
        assert_eq!(data.next_step(), NextStep::PayHoldInvoice);
        assert_eq!(data.payment_request(), Some("lnbc1hold"));
        assert_eq!(data.side(), Kind::Buy);
        assert_eq!(saved.status, Some(Status::WaitingPayment));
    }

    #[test]
    fn the_minimal_row_records_the_amount_taken_and_the_side_of_the_order() {
        let sell = input(TakeSide::Sell, |r| r.amount = Some(150)).minimal_order();
        assert_eq!((sell.kind, sell.fiat_amount), (Some(Kind::Sell), 150));
        let buy = input(TakeSide::Buy, |_| {}).minimal_order();
        assert_eq!((buy.kind, buy.fiat_amount), (Some(Kind::Buy), 0));
    }

    #[test]
    fn a_reply_about_another_order_or_without_an_id_cannot_be_used() {
        let mut other = node_order(Status::WaitingPayment);
        other.id = Some(Uuid::from_u128(10));
        assert!(map(TakeSide::Sell, &Taken::AddInvoice(other.clone())).is_none());
        assert!(map(TakeSide::Buy, &Taken::PayInvoice(other, "lnbc1".into())).is_none());
        let mut none = node_order(Status::WaitingPayment);
        none.id = None;
        assert!(map(TakeSide::Sell, &Taken::AddInvoice(none)).is_none());
    }

    const ERRORS: [ErrorCode; 15] = [
        ErrorCode::InvalidArguments,
        ErrorCode::ConfigurationMissing,
        ErrorCode::ConfigurationInvalid,
        ErrorCode::RelayConnectionFailed,
        ErrorCode::RelayReadFailed,
        ErrorCode::InternalError,
        ErrorCode::IdentityMissing,
        ErrorCode::IdentityUnreadable,
        ErrorCode::ProtocolUnsupported,
        ErrorCode::PowRequired,
        ErrorCode::RequestRejected,
        ErrorCode::NodeRefused,
        ErrorCode::OrderDeclined,
        ErrorCode::BondRequired,
        ErrorCode::StoreUnwritable,
    ];
    const UNCERTAIN: [ErrorCode; 6] = [
        ErrorCode::ReplyTimeout,
        ErrorCode::ReplyMalformed,
        ErrorCode::RequestRejected,
        ErrorCode::RelayReadFailed,
        ErrorCode::PowRequired,
        ErrorCode::OrderNotSaved,
    ];
    const WARNINGS: [WarningCode; 1] = [WarningCode::InvalidOrderTimestamp];

    fn schema(side: TakeSide) -> Value {
        Value::Object(output_schema::<OrderTakeData, ()>(&OutcomeRules::write(
            side.command(),
            &ERRORS,
            &UNCERTAIN,
            &WARNINGS,
        )))
    }

    fn ok(side: TakeSide, taken: &Taken, meta: ExecutionMeta<()>) -> Value {
        let (data, _, warnings) = map(side, taken).unwrap();
        serde_json::to_value(Envelope::ok(side.command(), data, warnings, meta)).unwrap()
    }

    fn accepted() -> ExecutionMeta<()> {
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Accepted,
            Persistence::Applied,
        )
    }

    #[test]
    fn each_command_has_its_own_schema_and_real_outcomes_conform() {
        for side in [TakeSide::Sell, TakeSide::Buy] {
            let schema = schema(side);
            assert_eq!(schema["title"], format!("{} outcome", side.command()));
            assert_eq!(
                schema["properties"]["status"]["enum"],
                json!(["ok", "error", "uncertain"])
            );
            for taken in [
                Taken::AddInvoice(node_order(Status::WaitingBuyerInvoice)),
                Taken::WaitingSellerToPay,
                Taken::PayInvoice(node_order(Status::WaitingPayment), "lnbc1hold".into()),
            ] {
                let outcome = ok(side, &taken, accepted());
                assert!(conforms(&schema, &schema, &outcome), "{outcome}");
            }
            let (data, ..) = map(side, &Taken::WaitingSellerToPay).unwrap();
            let unsaved = serde_json::to_value(Envelope::<OrderTakeData, ()>::uncertain(
                side.command(),
                ErrorCode::OrderNotSaved,
                Some(data),
                Warnings::new(),
                ExecutionMeta::write(
                    Publication::Published,
                    DaemonAcceptance::Accepted,
                    Persistence::Failed,
                ),
            ))
            .unwrap();
            assert!(conforms(&schema, &schema, &unsaved), "{unsaved}");
            assert_eq!(unsaved["status"], json!(OutcomeStatus::Uncertain));
        }
    }

    #[test]
    fn contradictory_take_outcomes_do_not_conform() {
        let schema = schema(TakeSide::Sell);
        let base = || ok(TakeSide::Sell, &Taken::WaitingSellerToPay, accepted());
        type Edit = Box<dyn Fn(&mut Value)>;
        let edits: Vec<(&str, Edit)> = vec![
            (
                "an unknown next step",
                Box::new(|o| o["data"]["next_step"] = json!("pay_now")),
            ),
            (
                "a numeric trade index",
                Box::new(|o| o["data"]["trade_index"] = json!(4)),
            ),
            (
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!("ABC")),
            ),
            (
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "a missing payment_request key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("payment_request");
                }),
            ),
            (
                "an unknown side",
                Box::new(|o| o["data"]["side"] = json!("both")),
            ),
            (
                "a failed save on success",
                Box::new(|o| o["meta"]["persistence"] = json!("failed")),
            ),
            (
                "another command",
                Box::new(|o| o["command"] = json!("order.take-buy")),
            ),
        ];
        for (label, edit) in edits {
            let mut outcome = base();
            edit(&mut outcome);
            assert!(!conforms(&schema, &schema, &outcome), "accepted: {label}");
        }
    }
}
