//! `order.add-invoice` and `bond.add-invoice` input and data: the invoice the
//! user sends for an order they already hold and what the node answered.
//!
//! Validation catches only what is cheap and certain; the node stays the
//! authority (who may answer, the amount, the order's state). A failed check
//! names the option, never its value. The node's reply is untrusted: it must
//! name the order and maps to explicit fields only. The invoice the user sent
//! appears nowhere in the data.

use std::fmt;

use mostro_core::order::{SmallOrder, Status};
use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::info::OrderRecord;
use super::{schema, ErrorCode, WarningCode, Warnings};

const INVOICE_BYTES: usize = 4096;

/// Codes that fail `order.add-invoice` with a plain error (nothing sent, or a
/// refusal). The ones that leave it `uncertain` are [`UNCERTAIN`].
pub const ORDER_ERRORS: [ErrorCode; 15] = [
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
    ErrorCode::StoreUnwritable,
    ErrorCode::OrderUnknown,
];

/// The same for `bond.add-invoice`, which writes nothing locally.
pub const BOND_ERRORS: [ErrorCode; 14] = [
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
    ErrorCode::OrderUnknown,
];

/// Codes that leave either command `uncertain`: the node may hold the invoice.
pub const UNCERTAIN: [ErrorCode; 5] = [
    ErrorCode::ReplyTimeout,
    ErrorCode::ReplyMalformed,
    ErrorCode::RequestRejected,
    ErrorCode::RelayReadFailed,
    ErrorCode::PowRequired,
];

/// Warnings of `order.add-invoice` beyond the pilot pair.
pub const ORDER_WARNINGS: [WarningCode; 2] = [
    WarningCode::LocalStatusStale,
    WarningCode::InvalidOrderTimestamp,
];

/// Which invoice is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceKind {
    /// The buyer's payout invoice (`order.add-invoice`): a BOLT-11 invoice or a
    /// Lightning address.
    Order,
    /// The payout of a slashed bond's share (`bond.add-invoice`): a BOLT-11
    /// invoice for the exact amount; an address cannot carry it.
    Bond,
}

impl InvoiceKind {
    pub fn command(self) -> &'static str {
        match self {
            Self::Order => "order.add-invoice",
            Self::Bond => "bond.add-invoice",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Self::Order => Action::AddInvoice,
            Self::Bond => Action::AddBondInvoice,
        }
    }
}

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidInvoice;

impl fmt::Display for InvalidInvoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid value for --invoice")
    }
}

impl std::error::Error for InvalidInvoice {}

/// What the user typed, before validation.
#[derive(Clone, Debug)]
pub struct InvoiceRequest {
    pub kind: InvoiceKind,
    pub order: Uuid,
    pub invoice: String,
}

/// A validated invoice for an order.
#[derive(Clone, PartialEq, Eq)]
pub struct InvoiceInput {
    kind: InvoiceKind,
    order: Uuid,
    invoice: String,
}

/// The invoice is the user's payout destination: `Debug` never shows it.
impl fmt::Debug for InvoiceInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvoiceInput")
            .field("kind", &self.kind)
            .field("order", &self.order)
            .finish()
    }
}

/// Whether `text` starts like a BOLT-11 invoice (`lnbc`, `lntb`, `lnsb`, and
/// their `rt`/`s` variants), in either case.
fn looks_like_bolt11(text: &str) -> bool {
    let head: String = text.chars().take(4).collect::<String>().to_lowercase();
    matches!(head.as_str(), "lnbc" | "lntb" | "lnsb")
}

fn looks_like_address(text: &str) -> bool {
    text.matches('@').count() == 1 && !text.starts_with('@') && !text.ends_with('@')
}

impl InvoiceInput {
    pub fn validate(request: InvoiceRequest) -> Result<Self, InvalidInvoice> {
        let invoice = request.invoice.trim().to_string();
        let shaped = match request.kind {
            InvoiceKind::Order => looks_like_bolt11(&invoice) || looks_like_address(&invoice),
            InvoiceKind::Bond => looks_like_bolt11(&invoice),
        };
        if !shaped
            || invoice.len() > INVOICE_BYTES
            || invoice.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(InvalidInvoice);
        }
        Ok(Self {
            kind: request.kind,
            order: request.order,
            invoice,
        })
    }

    /// The payload of the request: the invoice as given, in a bare payment
    /// request, which is what the node reads for both actions.
    pub fn payload(&self) -> Payload {
        Payload::PaymentRequest(None, self.invoice.clone(), None)
    }

    pub fn kind(&self) -> InvoiceKind {
        self.kind
    }

    pub fn order(&self) -> Uuid {
        self.order
    }
}

/// A reply of the node to `add-invoice`, with the payload it must carry.
#[derive(Clone, Debug)]
pub enum Answered {
    /// `waiting-seller-to-pay`: no payload.
    WaitingSellerToPay,
    /// `hold-invoice-payment-accepted` with the order.
    PaymentAccepted(Box<SmallOrder>),
    /// `invoice-updated`: no payload.
    InvoiceUpdated,
}

/// What the node did with the buyer's invoice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceResult {
    /// The invoice was accepted; the seller has to pay the hold invoice.
    WaitingSellerPayment,
    /// The invoice was accepted and the seller had already paid.
    PaymentAccepted,
    /// The replacement invoice was stored.
    InvoiceUpdated,
}

/// `order.add-invoice` data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderInvoiceData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    result: InvoiceResult,
    /// The node's order; null when the reply carried none.
    order: Option<OrderRecord>,
    /// The public key of the order's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl OrderInvoiceData {
    /// Map the node's `answered` reply for `input`. Returns the data, the status
    /// to record locally (`None` when the reply gives no new one) and the
    /// warnings; `None` when the reply names another order or carries no id.
    pub fn from_answer(
        node: &PublicKey,
        input: &InvoiceInput,
        answered: &Answered,
        trade_pubkey: &PublicKey,
    ) -> Option<(Self, Option<Status>, Warnings)> {
        let (result, node_order, status) = match answered {
            Answered::WaitingSellerToPay => (
                InvoiceResult::WaitingSellerPayment,
                None,
                Some(Status::WaitingPayment),
            ),
            Answered::PaymentAccepted(order) => {
                (InvoiceResult::PaymentAccepted, Some(&**order), order.status)
            }
            Answered::InvoiceUpdated => (InvoiceResult::InvoiceUpdated, None, None),
        };
        if node_order.is_some_and(|order| order.id != Some(input.order)) {
            return None;
        }
        let mut warnings = Warnings::new();
        let order =
            node_order.map(|order| OrderRecord::from_order(input.order, order, &mut warnings));
        let data = Self {
            node_pubkey: node.to_hex(),
            order_id: input.order.to_string(),
            result,
            order,
            trade_pubkey: trade_pubkey.to_hex(),
        };
        Some((data, status, warnings))
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn result(&self) -> InvoiceResult {
        self.result
    }

    pub fn order(&self) -> Option<&OrderRecord> {
        self.order.as_ref()
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }
}

/// What the node did with the bond payout invoice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BondResult {
    /// The invoice was accepted; the node pays it from its wallet.
    InvoiceAccepted,
}

/// `bond.add-invoice` data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BondInvoiceData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    result: BondResult,
    /// The share the node reported, in sats; null when it reported none.
    #[schemars(schema_with = "schema::nullable_decimal")]
    counterparty_share_sats: Option<String>,
    /// The public key of the order's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl BondInvoiceData {
    /// Map the node's `bond-invoice-accepted` order for `input`; `None` when it
    /// names another order or carries no id.
    pub fn from_ack(
        node: &PublicKey,
        input: &InvoiceInput,
        acknowledged: &SmallOrder,
        trade_pubkey: &PublicKey,
    ) -> Option<Self> {
        if acknowledged.id != Some(input.order) {
            return None;
        }
        Some(Self {
            node_pubkey: node.to_hex(),
            order_id: input.order.to_string(),
            result: BondResult::InvoiceAccepted,
            counterparty_share_sats: (acknowledged.amount > 0)
                .then(|| acknowledged.amount.to_string()),
            trade_pubkey: trade_pubkey.to_hex(),
        })
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn result(&self) -> BondResult {
        self.result
    }

    pub fn counterparty_share_sats(&self) -> Option<&str> {
        self.counterparty_share_sats.as_deref()
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
    use crate::outcome::schema::{output_schema, OutcomeRules, Stages};
    use crate::outcome::{
        DaemonAcceptance, Envelope, ExecutionMeta, Persistence, Publication,
        Status as OutcomeStatus,
    };
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};

    const BOLT11: &str = "lnbcrt1scriptedinvoice";

    fn order_id() -> Uuid {
        Uuid::from_u128(9)
    }

    fn request(kind: InvoiceKind, invoice: &str) -> InvoiceRequest {
        InvoiceRequest {
            kind,
            order: order_id(),
            invoice: invoice.to_string(),
        }
    }

    fn input(kind: InvoiceKind) -> InvoiceInput {
        InvoiceInput::validate(request(kind, BOLT11)).unwrap()
    }

    fn node_order(status: Status, amount: i64) -> SmallOrder {
        SmallOrder::new(
            Some(order_id()),
            Some(mostro_core::order::Kind::Sell),
            Some(status),
            amount,
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

    #[test]
    fn the_kinds_name_their_command_and_action() {
        assert_eq!(InvoiceKind::Order.command(), "order.add-invoice");
        assert_eq!(InvoiceKind::Bond.command(), "bond.add-invoice");
        assert_eq!(InvoiceKind::Order.action(), Action::AddInvoice);
        assert_eq!(InvoiceKind::Bond.action(), Action::AddBondInvoice);
    }

    #[test]
    fn an_order_invoice_is_a_bolt11_invoice_or_a_lightning_address() {
        for good in [
            "lnbc1abc",
            "LNBC1ABC",
            "lntb1abc",
            "lnbcrt1abc",
            "  lnbc1abc  ",
            "buyer@example.com",
        ] {
            assert!(
                InvoiceInput::validate(request(InvoiceKind::Order, good)).is_ok(),
                "{good}"
            );
        }
    }

    #[test]
    fn a_bond_invoice_is_a_bolt11_invoice_and_never_an_address() {
        assert!(InvoiceInput::validate(request(InvoiceKind::Bond, "lnbcrt1abc")).is_ok());
        assert!(InvoiceInput::validate(request(InvoiceKind::Bond, "buyer@example.com")).is_err());
    }

    #[test]
    fn what_is_neither_shaped_nor_safe_is_refused_without_echoing_it() {
        let too_long = format!("lnbc1{}", "a".repeat(INVOICE_BYTES));
        for bad in [
            "",
            "   ",
            "not an invoice",
            "lnbc1 abc",
            "ln\u{1b}[2J",
            "lnbc1\nabc",
            "@example.com",
            "buyer@",
            "a@b@c",
            "lnurl1abc",
            too_long.as_str(),
        ] {
            let error = InvoiceInput::validate(request(InvoiceKind::Order, bad)).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --invoice", "{bad:?}");
        }
        let error = InvoiceInput::validate(request(InvoiceKind::Order, "SENTINEL value"));
        assert!(!format!("{error:?}").contains("SENTINEL"));
    }

    #[test]
    fn the_payload_is_a_bare_payment_request_with_the_invoice_as_given() {
        for kind in [InvoiceKind::Order, InvoiceKind::Bond] {
            let payload = serde_json::to_value(input(kind).payload()).unwrap();
            assert_eq!(payload, json!({"payment_request": [null, BOLT11, null]}));
        }
    }

    #[test]
    fn debug_output_never_shows_the_invoice() {
        let shown = format!("{:?}", input(InvoiceKind::Order));
        assert!(!shown.contains(BOLT11), "{shown}");
    }

    fn map(answered: &Answered) -> Option<(OrderInvoiceData, Option<Status>, Warnings)> {
        let (node, trade) = keys();
        OrderInvoiceData::from_answer(&node, &input(InvoiceKind::Order), answered, &trade)
    }

    #[test]
    fn each_reply_maps_to_its_result_and_the_status_to_record() {
        let (data, status, warnings) = map(&Answered::WaitingSellerToPay).unwrap();
        assert_eq!(data.result(), InvoiceResult::WaitingSellerPayment);
        assert!(data.order().is_none());
        assert_eq!(status, Some(Status::WaitingPayment));
        assert!(warnings.is_empty());

        let (data, status, _) = map(&Answered::PaymentAccepted(Box::new(node_order(
            Status::Active,
            15_000,
        ))))
        .unwrap();
        assert_eq!(data.result(), InvoiceResult::PaymentAccepted);
        assert_eq!(data.order().unwrap().id(), order_id().to_string());
        assert_eq!(status, Some(Status::Active), "the status the node sent");

        let (data, status, _) = map(&Answered::InvoiceUpdated).unwrap();
        assert_eq!(data.result(), InvoiceResult::InvoiceUpdated);
        assert_eq!(status, None, "nothing new to record");
        assert_eq!(data.order_id(), order_id().to_string());
    }

    #[test]
    fn an_accepted_payment_without_a_status_records_nothing() {
        let mut order = node_order(Status::Active, 15_000);
        order.status = None;
        let (_, status, _) = map(&Answered::PaymentAccepted(Box::new(order))).unwrap();
        assert_eq!(status, None);
    }

    #[test]
    fn a_reply_about_another_order_or_without_an_id_cannot_be_used() {
        let mut other = node_order(Status::Active, 15_000);
        other.id = Some(Uuid::from_u128(10));
        assert!(map(&Answered::PaymentAccepted(Box::new(other))).is_none());
        let mut none = node_order(Status::Active, 15_000);
        none.id = None;
        assert!(map(&Answered::PaymentAccepted(Box::new(none))).is_none());
    }

    fn bond(order: &SmallOrder) -> Option<BondInvoiceData> {
        let (node, trade) = keys();
        BondInvoiceData::from_ack(&node, &input(InvoiceKind::Bond), order, &trade)
    }

    #[test]
    fn a_bond_ack_reports_the_share_when_the_node_gave_one() {
        let data = bond(&node_order(Status::Success, 5_000)).unwrap();
        assert_eq!(data.result(), BondResult::InvoiceAccepted);
        assert_eq!(data.counterparty_share_sats(), Some("5000"));
        assert_eq!(data.order_id(), order_id().to_string());

        let none = bond(&node_order(Status::Success, 0)).unwrap();
        assert_eq!(none.counterparty_share_sats(), None);
    }

    #[test]
    fn a_bond_ack_about_another_order_or_without_an_id_cannot_be_used() {
        let mut other = node_order(Status::Success, 5_000);
        other.id = Some(Uuid::from_u128(10));
        assert!(bond(&other).is_none());
        other.id = None;
        assert!(bond(&other).is_none());
    }

    fn order_schema() -> Value {
        Value::Object(output_schema::<OrderInvoiceData, ()>(
            &OutcomeRules::invoice(
                "order.add-invoice",
                Stages::Update,
                &ORDER_ERRORS,
                &UNCERTAIN,
                &ORDER_WARNINGS,
            ),
        ))
    }

    fn bond_schema() -> Value {
        Value::Object(output_schema::<BondInvoiceData, ()>(
            &OutcomeRules::invoice(
                "bond.add-invoice",
                Stages::Delivery,
                &BOND_ERRORS,
                &UNCERTAIN,
                &[],
            ),
        ))
    }

    fn meta(persistence: Persistence) -> ExecutionMeta<()> {
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Accepted,
            persistence,
        )
    }

    fn order_ok(answered: &Answered, persistence: Persistence) -> Value {
        let (data, _, warnings) = map(answered).unwrap();
        serde_json::to_value(Envelope::ok(
            "order.add-invoice",
            data,
            warnings,
            meta(persistence),
        ))
        .unwrap()
    }

    fn bond_ok() -> Value {
        let data = bond(&node_order(Status::Success, 5_000)).unwrap();
        serde_json::to_value(Envelope::ok(
            "bond.add-invoice",
            data,
            Warnings::new(),
            meta(Persistence::NotAttempted),
        ))
        .unwrap()
    }

    #[test]
    fn real_order_outcomes_conform_including_a_stale_local_status() {
        let schema = order_schema();
        assert_eq!(schema["title"], "order.add-invoice outcome");
        for answered in [
            Answered::WaitingSellerToPay,
            Answered::PaymentAccepted(Box::new(node_order(Status::Active, 15_000))),
            Answered::InvoiceUpdated,
        ] {
            for persistence in [Persistence::Applied, Persistence::Failed] {
                let outcome = order_ok(&answered, persistence);
                assert!(conforms(&schema, &schema, &outcome), "{outcome}");
            }
        }
        let (data, ..) = map(&Answered::InvoiceUpdated).unwrap();
        let mut warnings = Warnings::new();
        warnings.add(WarningCode::LocalStatusStale);
        let stale = serde_json::to_value(Envelope::ok(
            "order.add-invoice",
            data,
            warnings,
            meta(Persistence::Failed),
        ))
        .unwrap();
        assert!(conforms(&schema, &schema, &stale), "{stale}");
    }

    #[test]
    fn uncertain_and_error_outcomes_conform_and_carry_no_data() {
        for (schema, command) in [
            (order_schema(), "order.add-invoice"),
            (bond_schema(), "bond.add-invoice"),
        ] {
            for code in UNCERTAIN {
                let outcome = serde_json::to_value(Envelope::<Value, ()>::uncertain(
                    command,
                    code,
                    None,
                    Warnings::new(),
                    ExecutionMeta::write(
                        Publication::Published,
                        DaemonAcceptance::Unknown,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap();
                assert!(conforms(&schema, &schema, &outcome), "{command} {code:?}");
                assert_eq!(outcome["status"], json!(OutcomeStatus::Uncertain));
            }
            let refused = serde_json::to_value(Envelope::<Value, ()>::refused(
                Some(command),
                "invalid_invoice",
                Warnings::new(),
                ExecutionMeta::write(
                    Publication::Published,
                    DaemonAcceptance::Rejected,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap();
            assert!(conforms(&schema, &schema, &refused), "{command}");
            let unknown = serde_json::to_value(Envelope::<Value, ()>::error(
                Some(command),
                ErrorCode::OrderUnknown,
                Warnings::new(),
                ExecutionMeta::write(
                    Publication::NotAttempted,
                    DaemonAcceptance::Unknown,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap();
            assert!(conforms(&schema, &schema, &unknown), "{command}");
        }
    }

    #[test]
    fn a_real_bond_outcome_conforms_and_only_with_no_local_write() {
        let schema = bond_schema();
        assert!(conforms(&schema, &schema, &bond_ok()));
        let mut written = bond_ok();
        written["meta"]["persistence"] = json!("applied");
        assert!(
            !conforms(&schema, &schema, &written),
            "a bond writes nothing"
        );
    }

    #[test]
    fn the_bond_command_has_no_store_code_and_the_order_command_has_no_share() {
        let codes = |schema: &Value| -> Vec<String> {
            schema["$defs"]["ErrorCode"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|code| code.as_str().map(str::to_owned))
                .collect()
        };
        assert!(codes(&order_schema()).contains(&"store_unwritable".to_string()));
        assert!(!codes(&bond_schema()).contains(&"store_unwritable".to_string()));
        assert!(codes(&bond_schema()).contains(&"order_unknown".to_string()));
        assert!(order_schema()["$defs"].get("BondInvoiceData").is_none());
    }

    #[test]
    fn contradictory_outcomes_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let edits: Vec<(&str, Edit)> = vec![
            (
                "an unknown result",
                Box::new(|o| o["data"]["result"] = json!("paid")),
            ),
            (
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!("ABC")),
            ),
            (
                "a missing order key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("order");
                }),
            ),
            (
                "another command",
                Box::new(|o| o["command"] = json!("bond.add-invoice")),
            ),
        ];
        let schema = order_schema();
        for (label, edit) in edits {
            let mut outcome = order_ok(&Answered::WaitingSellerToPay, Persistence::Applied);
            assert!(
                outcome["data"].get("trade_index").is_none(),
                "no index is spent"
            );
            edit(&mut outcome);
            assert!(!conforms(&schema, &schema, &outcome), "{label}");
        }
    }
}
