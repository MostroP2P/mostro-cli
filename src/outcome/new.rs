//! `order.new` input and data: the order the user asks for and the order the
//! node created.
//!
//! Validation here catches only what is cheap and certain; the node stays the
//! authority (accepted currencies, quotes, invoice amounts). A failed check
//! names the option, never its value. The node's reply is untrusted input and
//! maps to explicit fields only.

use std::fmt;

use mostro_core::order::{Kind, SmallOrder, Status};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::info::OrderRecord;
use super::{schema, Warnings};

const CURRENCY_LENGTH: std::ops::RangeInclusive<usize> = 3..=6;
const PAYMENT_METHOD_BYTES: usize = 200;
const INVOICE_BYTES: usize = 4096;
const PREMIUM_PERCENT: std::ops::RangeInclusive<i64> = -100..=100;
const EXPIRATION_DAYS: std::ops::RangeInclusive<i64> = 0..=365;
const SECONDS_PER_DAY: i64 = 86_400;

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidOrder {
    Currency,
    FiatAmount,
    FiatRange,
    PaymentMethod,
    Sats,
    Premium,
    Invoice,
    ExpirationDays,
}

impl InvalidOrder {
    /// The long option to blame, fixed text.
    pub fn option(self) -> &'static str {
        match self {
            Self::Currency => "--currency",
            Self::FiatAmount => "--fiat-amount",
            Self::FiatRange => "--fiat-amount-max",
            Self::PaymentMethod => "--payment-method",
            Self::Sats => "--sats",
            Self::Premium => "--premium",
            Self::Invoice => "--invoice",
            Self::ExpirationDays => "--expiration-days",
        }
    }
}

impl fmt::Display for InvalidOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid value for {}", self.option())
    }
}

impl std::error::Error for InvalidOrder {}

/// What the user typed, before validation.
#[derive(Clone, Debug)]
pub struct OrderRequest {
    pub side: Kind,
    pub currency: String,
    pub fiat_amount: i64,
    pub fiat_amount_max: Option<i64>,
    pub payment_method: String,
    pub sats: i64,
    pub premium: i64,
    pub invoice: Option<String>,
    pub expiration_days: i64,
}

/// A validated order request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderNewInput {
    side: Kind,
    currency: String,
    fiat_amount: i64,
    fiat_amount_max: Option<i64>,
    payment_method: String,
    sats: i64,
    premium: i64,
    invoice: Option<String>,
    expiration_days: i64,
}

impl OrderNewInput {
    pub fn validate(request: OrderRequest) -> Result<Self, InvalidOrder> {
        let currency = request.currency.trim().to_ascii_uppercase();
        if !CURRENCY_LENGTH.contains(&currency.len())
            || !currency.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(InvalidOrder::Currency);
        }
        if request.fiat_amount <= 0 {
            return Err(InvalidOrder::FiatAmount);
        }
        // A range is `min..max` with a market price: the node refuses a fixed
        // sats amount on a range.
        if let Some(max) = request.fiat_amount_max {
            if max <= request.fiat_amount || request.sats != 0 {
                return Err(InvalidOrder::FiatRange);
            }
        }
        let method = request.payment_method.trim();
        if method.is_empty()
            || method.len() > PAYMENT_METHOD_BYTES
            || method.chars().any(char::is_control)
        {
            return Err(InvalidOrder::PaymentMethod);
        }
        if request.sats < 0 {
            return Err(InvalidOrder::Sats);
        }
        if !PREMIUM_PERCENT.contains(&request.premium) {
            return Err(InvalidOrder::Premium);
        }
        let invoice = request.invoice.map(|text| text.trim().to_string());
        if invoice.as_ref().is_some_and(|text| {
            text.is_empty()
                || text.len() > INVOICE_BYTES
                || text.chars().any(|c| c.is_control() || c.is_whitespace())
        }) {
            return Err(InvalidOrder::Invoice);
        }
        if !EXPIRATION_DAYS.contains(&request.expiration_days) {
            return Err(InvalidOrder::ExpirationDays);
        }
        Ok(Self {
            side: request.side,
            currency,
            fiat_amount: request.fiat_amount,
            fiat_amount_max: request.fiat_amount_max,
            payment_method: method.to_string(),
            sats: request.sats,
            premium: request.premium,
            invoice,
            expiration_days: request.expiration_days,
        })
    }

    /// The order as the node expects it in a `new-order`: pending, no id, a
    /// fixed amount in `fiat_amount` or a range in `min`/`max` with
    /// `fiat_amount` 0, and an absolute expiry `now + days` when days are given.
    pub fn small_order(&self, now: i64) -> SmallOrder {
        let (fiat_amount, min, max) = match self.fiat_amount_max {
            Some(max) => (0, Some(self.fiat_amount), Some(max)),
            None => (self.fiat_amount, None, None),
        };
        SmallOrder::new(
            None,
            Some(self.side),
            Some(Status::Pending),
            self.sats,
            self.currency.clone(),
            min,
            max,
            fiat_amount,
            self.payment_method.clone(),
            self.premium,
            None,
            None,
            self.invoice.clone(),
            Some(0),
            (self.expiration_days > 0).then(|| now + self.expiration_days * SECONDS_PER_DAY),
        )
    }

    pub fn side(&self) -> Kind {
        self.side
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn fiat_amount(&self) -> i64 {
        self.fiat_amount
    }

    pub fn fiat_amount_max(&self) -> Option<i64> {
        self.fiat_amount_max
    }

    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }

    pub fn sats(&self) -> i64 {
        self.sats
    }

    pub fn premium(&self) -> i64 {
        self.premium
    }

    pub fn has_invoice(&self) -> bool {
        self.invoice.is_some()
    }

    pub fn expiration_days(&self) -> i64 {
        self.expiration_days
    }
}

/// `order.new` success data (and the data of an `order_not_saved` outcome):
/// the order the node created and the key slot it was created under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderNewData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    order: OrderRecord,
    #[schemars(schema_with = "schema::decimal")]
    trade_index: String,
    /// The public key of the order's trade key; the private key stays
    /// derivable from the mnemonic and the index and is never shown.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl OrderNewData {
    /// Map the `created` order of the node's acknowledgement; `None` when it
    /// carries no id (the reply cannot be used). The order id comes from the
    /// node and is kept as given when it parses.
    pub fn from_created(
        node: &PublicKey,
        created: &SmallOrder,
        trade_index: i64,
        trade_pubkey: &PublicKey,
    ) -> Option<(Self, Warnings)> {
        let id: Uuid = created.id?;
        let mut warnings = Warnings::new();
        let order = OrderRecord::from_order(id, created, &mut warnings);
        let data = Self {
            node_pubkey: node.to_hex(),
            order,
            trade_index: trade_index.to_string(),
            trade_pubkey: trade_pubkey.to_hex(),
        };
        Some((data, warnings))
    }

    pub fn order(&self) -> &OrderRecord {
        &self.order
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

    fn request() -> OrderRequest {
        OrderRequest {
            side: Kind::Sell,
            currency: "usd".to_string(),
            fiat_amount: 100,
            fiat_amount_max: None,
            payment_method: "bank transfer".to_string(),
            sats: 0,
            premium: 2,
            invoice: None,
            expiration_days: 0,
        }
    }

    fn invalid(change: impl FnOnce(&mut OrderRequest)) -> InvalidOrder {
        let mut request = request();
        change(&mut request);
        OrderNewInput::validate(request).unwrap_err()
    }

    #[test]
    fn a_valid_request_is_normalised() {
        let mut raw = request();
        raw.payment_method = "  SEPA  ".to_string();
        raw.invoice = Some(" lnbc1abc ".to_string());
        let input = OrderNewInput::validate(raw).unwrap();
        assert_eq!(input.currency(), "USD");
        assert_eq!(input.payment_method(), "SEPA");
        assert!(input.has_invoice());
        assert_eq!(
            (input.sats(), input.premium(), input.expiration_days()),
            (0, 2, 0)
        );
    }

    #[test]
    fn every_rule_names_only_its_option() {
        assert_eq!(
            invalid(|r| r.currency = "US".into()),
            InvalidOrder::Currency
        );
        assert_eq!(
            invalid(|r| r.currency = "TOOLONG".into()),
            InvalidOrder::Currency
        );
        assert_eq!(
            invalid(|r| r.currency = "U$D".into()),
            InvalidOrder::Currency
        );
        assert_eq!(invalid(|r| r.fiat_amount = 0), InvalidOrder::FiatAmount);
        assert_eq!(invalid(|r| r.fiat_amount = -5), InvalidOrder::FiatAmount);
        assert_eq!(
            invalid(|r| r.fiat_amount_max = Some(100)),
            InvalidOrder::FiatRange
        );
        assert_eq!(
            invalid(|r| r.fiat_amount_max = Some(50)),
            InvalidOrder::FiatRange
        );
        assert_eq!(
            invalid(|r| {
                r.fiat_amount_max = Some(200);
                r.sats = 1_000;
            }),
            InvalidOrder::FiatRange,
            "a range is market-priced"
        );
        assert_eq!(
            invalid(|r| r.payment_method = " ".into()),
            InvalidOrder::PaymentMethod
        );
        assert_eq!(
            invalid(|r| r.payment_method = "a\u{1b}[2J".into()),
            InvalidOrder::PaymentMethod
        );
        assert_eq!(
            invalid(|r| r.payment_method = "x".repeat(201)),
            InvalidOrder::PaymentMethod
        );
        assert_eq!(invalid(|r| r.sats = -1), InvalidOrder::Sats);
        assert_eq!(invalid(|r| r.premium = 101), InvalidOrder::Premium);
        assert_eq!(invalid(|r| r.premium = -101), InvalidOrder::Premium);
        assert_eq!(
            invalid(|r| r.invoice = Some("".into())),
            InvalidOrder::Invoice
        );
        assert_eq!(
            invalid(|r| r.invoice = Some("ln bc".into())),
            InvalidOrder::Invoice
        );
        assert_eq!(
            invalid(|r| r.invoice = Some("x".repeat(4097))),
            InvalidOrder::Invoice
        );
        assert_eq!(
            invalid(|r| r.expiration_days = -1),
            InvalidOrder::ExpirationDays
        );
        assert_eq!(
            invalid(|r| r.expiration_days = 366),
            InvalidOrder::ExpirationDays
        );
        // Bounds are inclusive and a secret-looking value is never echoed.
        let mut edge = request();
        edge.premium = -100;
        edge.expiration_days = 365;
        edge.currency = "ABC123".into();
        assert!(OrderNewInput::validate(edge).is_ok());
        let error = invalid(|r| r.payment_method = "SENTINEL\u{7}".into());
        assert!(!error.to_string().contains("SENTINEL"));
        assert_eq!(error.to_string(), "invalid value for --payment-method");
    }

    #[test]
    fn the_node_order_has_the_legacy_shape_for_fixed_and_range_orders() {
        let mut raw = request();
        raw.invoice = Some("lnbc1".into());
        raw.expiration_days = 2;
        let fixed = OrderNewInput::validate(raw).unwrap().small_order(1_000);
        assert_eq!(fixed.id, None);
        assert_eq!(
            (fixed.kind, fixed.status),
            (Some(Kind::Sell), Some(Status::Pending))
        );
        assert_eq!((fixed.amount, fixed.fiat_amount), (0, 100));
        assert_eq!((fixed.min_amount, fixed.max_amount), (None, None));
        assert_eq!(fixed.buyer_invoice.as_deref(), Some("lnbc1"));
        assert_eq!(fixed.expires_at, Some(1_000 + 2 * 86_400));
        assert_eq!(fixed.created_at, Some(0));

        let mut raw = request();
        raw.fiat_amount_max = Some(300);
        let range = OrderNewInput::validate(raw).unwrap().small_order(1_000);
        assert_eq!(range.fiat_amount, 0, "a range carries no single amount");
        assert_eq!((range.min_amount, range.max_amount), (Some(100), Some(300)));
        assert_eq!(range.expires_at, None, "zero days is the node's default");
    }

    fn created(id: Option<Uuid>) -> SmallOrder {
        let mut order = OrderNewInput::validate(request()).unwrap().small_order(0);
        order.id = id;
        order.created_at = Some(1_792_108_800);
        order.expires_at = Some(1_792_195_200);
        order
    }

    fn data() -> (OrderNewData, Warnings) {
        let (node, trade) = (Keys::generate().public_key(), Keys::generate().public_key());
        OrderNewData::from_created(&node, &created(Some(Uuid::from_u128(7))), 3, &trade).unwrap()
    }

    #[test]
    fn the_acknowledgement_maps_to_explicit_fields_and_needs_an_id() {
        let (data, warnings) = data();
        assert!(warnings.is_empty());
        assert_eq!(data.order().id(), Uuid::from_u128(7).to_string());
        assert_eq!(data.trade_index(), "3");
        assert_eq!(data.order().created_at_unix(), Some("1792108800"));
        let (node, trade) = (Keys::generate().public_key(), Keys::generate().public_key());
        assert!(OrderNewData::from_created(&node, &created(None), 3, &trade).is_none());
    }

    const ERRORS: [ErrorCode; 13] = [
        ErrorCode::IdentityMissing,
        ErrorCode::IdentityUnreadable,
        ErrorCode::ConfigurationMissing,
        ErrorCode::ConfigurationInvalid,
        ErrorCode::ProtocolUnsupported,
        ErrorCode::PowRequired,
        ErrorCode::RequestRejected,
        ErrorCode::NodeRefused,
        ErrorCode::RelayConnectionFailed,
        ErrorCode::RelayReadFailed,
        ErrorCode::OrderDeclined,
        ErrorCode::BondRequired,
        ErrorCode::StoreUnwritable,
    ];
    const UNCERTAIN: [ErrorCode; 5] = [
        ErrorCode::ReplyTimeout,
        ErrorCode::ReplyMalformed,
        ErrorCode::RequestRejected,
        ErrorCode::RelayReadFailed,
        ErrorCode::OrderNotSaved,
    ];
    const WARNINGS: [WarningCode; 1] = [WarningCode::InvalidOrderTimestamp];

    fn schema() -> Value {
        Value::Object(output_schema::<OrderNewData, ()>(&OutcomeRules::write(
            "order.new",
            &ERRORS,
            &UNCERTAIN,
            &WARNINGS,
        )))
    }

    fn meta(p: Publication, d: DaemonAcceptance, s: Persistence) -> ExecutionMeta<()> {
        ExecutionMeta::write(p, d, s)
    }

    fn ok(meta: ExecutionMeta<()>) -> Value {
        let (data, warnings) = data();
        serde_json::to_value(Envelope::ok("order.new", data, warnings, meta)).unwrap()
    }

    fn error(code: ErrorCode, meta: ExecutionMeta<()>) -> Value {
        let envelope =
            Envelope::<OrderNewData, ()>::error(Some("order.new"), code, Warnings::new(), meta);
        serde_json::to_value(envelope).unwrap()
    }

    fn uncertain(code: ErrorCode, with_data: bool, meta: ExecutionMeta<()>) -> Value {
        let data = with_data.then(|| data().0);
        let envelope =
            Envelope::<OrderNewData, ()>::uncertain("order.new", code, data, Warnings::new(), meta);
        assert_eq!(envelope.status(), OutcomeStatus::Uncertain);
        serde_json::to_value(envelope).unwrap()
    }

    use Persistence::{Applied, Failed, NotAttempted};

    #[test]
    fn the_schema_lists_the_uncertain_status_and_the_scoped_codes() {
        let schema = schema();
        assert_eq!(
            schema["properties"]["status"]["enum"],
            json!(["ok", "error", "uncertain"])
        );
        let codes: Vec<&str> = schema["$defs"]["ErrorCode"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for code in [
            "order_declined",
            "bond_required",
            "order_not_saved",
            "store_unwritable",
            "reply_timeout",
        ] {
            assert!(codes.contains(&code), "{code}");
        }
        assert_eq!(
            &codes[..9],
            [
                "invalid_arguments",
                "output_not_supported",
                "configuration_missing",
                "configuration_invalid",
                "relay_connection_failed",
                "relay_read_failed",
                "read_timeout",
                "read_limit_exceeded",
                "internal_error"
            ]
        );
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 4);
        // A command without uncertain outcomes keeps three branches and two statuses.
        let read = Value::Object(output_schema::<OrderNewData, ()>(&OutcomeRules::request(
            "order.info",
            &ERRORS,
            &WARNINGS,
        )));
        assert_eq!(read["properties"]["status"]["enum"], json!(["ok", "error"]));
        assert_eq!(read["oneOf"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn real_write_outcomes_of_every_kind_conform() {
        let schema = schema();
        let sent = || {
            meta(
                Publication::NotAttempted,
                DaemonAcceptance::Unknown,
                NotAttempted,
            )
        };
        let refused = meta(
            Publication::Published,
            DaemonAcceptance::Rejected,
            NotAttempted,
        );
        let cases = [
            ok(meta(
                Publication::Published,
                DaemonAcceptance::Accepted,
                Applied,
            )),
            error(ErrorCode::IdentityMissing, sent()),
            error(ErrorCode::OrderDeclined, sent()),
            error(ErrorCode::StoreUnwritable, sent()),
            error(ErrorCode::RequestRejected, sent()),
            error(
                ErrorCode::BondRequired,
                meta(Publication::Published, DaemonAcceptance::Accepted, Applied),
            ),
            error(
                ErrorCode::BondRequired,
                meta(Publication::Published, DaemonAcceptance::Accepted, Failed),
            ),
            serde_json::to_value(Envelope::<OrderNewData, ()>::refused(
                Some("order.new"),
                "invalid_amount",
                Warnings::new(),
                refused,
            ))
            .unwrap(),
            uncertain(
                ErrorCode::OrderNotSaved,
                true,
                meta(Publication::Published, DaemonAcceptance::Accepted, Failed),
            ),
            uncertain(
                ErrorCode::ReplyTimeout,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            uncertain(
                ErrorCode::ReplyTimeout,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Failed),
            ),
            uncertain(
                ErrorCode::ReplyMalformed,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            uncertain(
                ErrorCode::RequestRejected,
                false,
                meta(Publication::Unknown, DaemonAcceptance::Unknown, Applied),
            ),
            uncertain(
                ErrorCode::RelayReadFailed,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            serde_json::to_value(Envelope::<OrderNewData, ()>::error(
                None,
                ErrorCode::InvalidArguments,
                Warnings::new(),
                ExecutionMeta::not_executed(),
            ))
            .unwrap(),
        ];
        for case in cases {
            assert!(conforms(&schema, &schema, &case), "{case}");
        }
    }

    #[test]
    fn contradictory_write_outcomes_do_not_conform() {
        let schema = schema();
        let accepted = || meta(Publication::Published, DaemonAcceptance::Accepted, Applied);
        let bad = [
            // A success needs an accepted, applied write.
            ok(meta(
                Publication::Published,
                DaemonAcceptance::Accepted,
                Failed,
            )),
            ok(meta(
                Publication::Published,
                DaemonAcceptance::Unknown,
                Applied,
            )),
            ok(meta(
                Publication::NotAttempted,
                DaemonAcceptance::Unknown,
                NotAttempted,
            )),
            // An error cannot be unknown after publication: that is uncertain.
            error(
                ErrorCode::ReplyTimeout,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            // Codes belong to their status.
            error(ErrorCode::OrderNotSaved, accepted()),
            uncertain(
                ErrorCode::NodeRefused,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            uncertain(
                ErrorCode::OrderDeclined,
                false,
                meta(Publication::Published, DaemonAcceptance::Unknown, Applied),
            ),
            // Uncertain needs a write that was attempted or an unknown node.
            uncertain(
                ErrorCode::ReplyTimeout,
                false,
                meta(
                    Publication::NotAttempted,
                    DaemonAcceptance::Unknown,
                    NotAttempted,
                ),
            ),
            uncertain(ErrorCode::OrderNotSaved, true, accepted()),
            // Request-only stage sets do not describe a write.
            ok(ExecutionMeta::request(
                Publication::Published,
                DaemonAcceptance::Accepted,
            )),
        ];
        for case in bad {
            assert!(!conforms(&schema, &schema, &case), "accepted: {case}");
        }
        let mut with_status = ok(accepted());
        with_status["status"] = json!("uncertain");
        assert!(!conforms(&schema, &schema, &with_status));
        let mut bad_index = ok(accepted());
        bad_index["data"]["trade_index"] = json!(3);
        assert!(!conforms(&schema, &schema, &bad_index));
        let mut bad_key = ok(accepted());
        bad_key["data"]["trade_pubkey"] = json!("ABC");
        assert!(!conforms(&schema, &schema, &bad_key));
    }
}
