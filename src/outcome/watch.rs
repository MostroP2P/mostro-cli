//! `order.watch` input and data: the contract of a watch on one order (spec
//! 020), emitted by `order watch` in human mode and with `--output json`.
//!
//! A watch follows the node's messages to the order's trade key and, once the
//! counterparty is known, the order's peer chat: the window is replayed first,
//! then new records arrive live. With `--output json` a call returns one
//! document, at the first new records or empty at its deadline, with a
//! [`Cursor`] that the caller passes back with `--after`. Human mode streams
//! updates until the trade ends. Both render the same [`WatchData`]. Nothing is
//! published, so every stage is `not_applicable`.
//!
//! Chat text and the text in node messages are untrusted data. Their records
//! bound them and the human renderer escapes them; they never appear in an
//! error.

use std::fmt;

use mostro_core::prelude::Action;
use nostr_sdk::prelude::PublicKey;
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use uuid::Uuid;

use super::chat::ChatRecord;
use super::messages::{MessageRecord, TradeRole, DEFAULT_SINCE_MINUTES, MAX_SINCE_MINUTES};
use super::rules::OutcomeRules;
use super::{schema, ErrorCode};

mod cursor;

pub use cursor::{
    Cursor, Observed, CURSOR_MARGIN_SECS, CURSOR_PATTERN, MAX_CURSOR_IDS, MAX_CURSOR_LEN,
};

/// Canonical name of `order watch`.
pub const COMMAND: &str = "order.watch";
/// The deadline of a JSON call without `--timeout`. Human mode has none.
pub const JSON_TIMEOUT_SECS: u64 = 30;
/// The longest `--timeout`: one day.
pub const MAX_TIMEOUT_SECS: i64 = 86_400;

/// Codes that fail `order.watch`.
pub const ERRORS: [ErrorCode; 12] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
    ErrorCode::StoreUnreadable,
    ErrorCode::OrderUnknown,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::ReadTimeout,
    ErrorCode::ReadLimitExceeded,
    ErrorCode::InternalError,
];

/// What `order.watch`'s outcomes may contain: a read, with only the pilot's
/// warnings. The counterparty key that the lookup may cache in the order row
/// is not reported, as for `order.chat`.
pub fn watch_rules() -> OutcomeRules<'static> {
    OutcomeRules::read(COMMAND, &ERRORS, false)
}

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidWatch {
    Since,
    After,
    /// Both `--since` and `--after` (the grammar refuses this first).
    SinceWithAfter,
    Timeout,
}

impl fmt::Display for InvalidWatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Since => "invalid value for --since",
            Self::After => "invalid value for --after",
            Self::SinceWithAfter => "--since cannot be used with --after",
            Self::Timeout => "invalid value for --timeout",
        })
    }
}

impl std::error::Error for InvalidWatch {}

/// A validated `order watch`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchInput {
    order: Uuid,
    start: Start,
    timeout_secs: Option<u64>,
}

/// Where a watch starts.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Start {
    /// The window, in minutes before now.
    Minutes(i64),
    /// Where an earlier call stopped.
    After(Cursor),
}

impl WatchInput {
    /// The window is 1 to [`MAX_SINCE_MINUTES`] minutes (default 30); `after`
    /// must be a [`Cursor`]; `timeout` is 1 to [`MAX_TIMEOUT_SECS`] seconds,
    /// and when it is not given a JSON call (`json`) waits
    /// [`JSON_TIMEOUT_SECS`] and human mode has no overall deadline.
    pub fn validate(
        order: Uuid,
        since: Option<i64>,
        after: Option<&str>,
        timeout: Option<i64>,
        json: bool,
    ) -> Result<Self, InvalidWatch> {
        let start = match (since, after) {
            (Some(_), Some(_)) => return Err(InvalidWatch::SinceWithAfter),
            (None, Some(text)) => Start::After(Cursor::parse(text)?),
            (since, None) => {
                let minutes = since.unwrap_or(DEFAULT_SINCE_MINUTES);
                if !(1..=MAX_SINCE_MINUTES).contains(&minutes) {
                    return Err(InvalidWatch::Since);
                }
                Start::Minutes(minutes)
            }
        };
        let timeout_secs = match timeout {
            Some(secs) if (1..=MAX_TIMEOUT_SECS).contains(&secs) => Some(secs.unsigned_abs()),
            Some(_) => return Err(InvalidWatch::Timeout),
            None => json.then_some(JSON_TIMEOUT_SECS),
        };
        Ok(Self {
            order,
            start,
            timeout_secs,
        })
    }

    pub fn order(&self) -> Uuid {
        self.order
    }

    /// The window in minutes; `None` with `--after`.
    pub fn since_minutes(&self) -> Option<i64> {
        match self.start {
            Start::Minutes(minutes) => Some(minutes),
            Start::After(_) => None,
        }
    }

    pub fn after(&self) -> Option<&Cursor> {
        match &self.start {
            Start::After(cursor) => Some(cursor),
            Start::Minutes(_) => None,
        }
    }

    /// The overall deadline in seconds, counted from the end of the first
    /// replay; `None` for a human watch without `--timeout`.
    pub fn timeout_secs(&self) -> Option<u64> {
        self.timeout_secs
    }

    /// Where the watch starts at `now` (Unix seconds): the cursor given, or
    /// the window before `now` with no ids.
    pub fn start_cursor(&self, now: u64) -> Cursor {
        match &self.start {
            Start::After(cursor) => cursor.clone(),
            Start::Minutes(minutes) => {
                Cursor::start(now.saturating_sub(minutes.unsigned_abs() * 60))
            }
        }
    }
}

/// Whether a node message with `action`, about the watched order, ends its
/// trade for a user on side `role` (`None` when the local row does not say).
/// `admin-settled` ends it only for the seller: the node pays the buyer after
/// it, and that payment can still fail and ask for a new invoice. `rate`, which
/// follows the end of a completed trade, ends it too.
pub fn ends_trade(action: &Action, role: Option<TradeRole>) -> bool {
    match action {
        Action::PurchaseCompleted
        | Action::HoldInvoicePaymentSettled
        | Action::Rate
        | Action::Canceled
        | Action::CooperativeCancelAccepted
        | Action::AdminCanceled => true,
        Action::AdminSettled => role == Some(TradeRole::Seller),
        _ => false,
    }
}

/// Whether the node sends `rate` right after `action`: it does after
/// `purchase-completed` (to the buyer) and `hold-invoice-payment-settled` (to
/// the seller), so a watch that sees one waits a short while for the `rate`.
pub fn rate_follows(action: &Action) -> bool {
    matches!(
        action,
        Action::PurchaseCompleted | Action::HoldInvoicePaymentSettled
    )
}

/// One `order.watch` result: the document of a JSON call, or one update of a
/// human watch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct WatchData {
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    /// The counterparty's trade key; null while the chat is not followed.
    #[schemars(schema_with = "schema::nullable_pubkey")]
    counterparty_pubkey: Option<String>,
    /// The node's new messages about the order, oldest first.
    messages: Vec<MessageRecord>,
    /// The new chat messages, oldest first, each once by its `message_id`.
    chat: Vec<ChatRecord>,
    /// One of the messages ended the trade.
    ended: bool,
    /// Older records may be missing: a relay's replay reached its limit.
    truncated: bool,
    /// Pass it back verbatim with `--after` to get only newer records.
    #[schemars(schema_with = "cursor_schema")]
    cursor: String,
}

fn cursor_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": CURSOR_PATTERN, "maxLength": MAX_CURSOR_LEN})
}

impl WatchData {
    pub fn new(
        order: Uuid,
        counterparty: Option<&PublicKey>,
        (messages, chat): (Vec<MessageRecord>, Vec<ChatRecord>),
        ended: bool,
        truncated: bool,
        cursor: &Cursor,
    ) -> Self {
        Self {
            order_id: order.to_string(),
            counterparty_pubkey: counterparty.map(PublicKey::to_hex),
            messages,
            chat,
            ended,
            truncated,
            cursor: cursor.to_string(),
        }
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn counterparty_pubkey(&self) -> Option<&str> {
        self.counterparty_pubkey.as_deref()
    }

    pub fn messages(&self) -> &[MessageRecord] {
        &self.messages
    }

    pub fn chat(&self) -> &[ChatRecord] {
        &self.chat
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn cursor(&self) -> &str {
        &self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::chat::{ChatRecord, ChatSender};
    use crate::outcome::messages::{MessageRecord, TradeRole};
    use crate::outcome::schema::output_schema;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, WarningCode, Warnings};
    use mostro_core::prelude::{Action, Message};
    use nostr_sdk::prelude::{EventId, Keys};
    use serde_json::{json, Value};
    use uuid::Uuid;

    const ORDER: Uuid = Uuid::from_u128(0x0a7c4);
    const NOW: u64 = 1_792_108_800;
    const SENTINEL: &str = "secret-sentinel";

    fn id(n: u8) -> EventId {
        EventId::from_byte_array([n; 32])
    }

    fn validate(
        since: Option<i64>,
        after: Option<&str>,
        timeout: Option<i64>,
        json: bool,
    ) -> Result<WatchInput, InvalidWatch> {
        WatchInput::validate(ORDER, since, after, timeout, json)
    }

    #[test]
    fn the_options_are_bounded_and_the_json_timeout_defaults_to_30() {
        let json = validate(None, None, None, true).unwrap();
        assert_eq!(JSON_TIMEOUT_SECS, 30);
        assert_eq!(
            (json.order(), json.since_minutes(), json.after()),
            (ORDER, Some(30), None)
        );
        assert_eq!(json.timeout_secs(), Some(30));
        assert_eq!(json.start_cursor(NOW), Cursor::start(NOW - 30 * 60));
        let human = validate(None, None, None, false).unwrap();
        assert_eq!(human.timeout_secs(), None, "no overall deadline");

        for good in [1, 10_080] {
            let input = validate(Some(good), None, None, true).unwrap();
            assert_eq!(input.since_minutes(), Some(good));
        }
        for bad in [0, -1, 10_081, i64::MIN, i64::MAX] {
            let error = validate(Some(bad), None, None, true).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --since", "{bad}");
        }
        for good in [1, 86_400] {
            for json in [true, false] {
                let input = validate(None, None, Some(good), json).unwrap();
                assert_eq!(input.timeout_secs(), u64::try_from(good).ok());
            }
        }
        for bad in [0, -1, 86_401, i64::MIN, i64::MAX] {
            let error = validate(None, None, Some(bad), false).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --timeout", "{bad}");
        }

        let after = validate(None, Some("1792107000"), Some(5), true).unwrap();
        assert_eq!(after.since_minutes(), None);
        assert_eq!(after.after().map(Cursor::since), Some(1_792_107_000));
        assert_eq!(after.start_cursor(NOW).since(), 1_792_107_000);
        let both = validate(Some(30), Some("5"), None, true).unwrap_err();
        assert_eq!(both, InvalidWatch::SinceWithAfter);

        let bad_cursor = format!("{SENTINEL}:{}", id(1).to_hex());
        let error = validate(None, Some(&bad_cursor), None, true).unwrap_err();
        let shown = format!("{error} {error:?}");
        assert_eq!(error.to_string(), "invalid value for --after");
        assert!(!shown.contains(SENTINEL), "{shown}");
    }

    #[test]
    fn only_the_ending_actions_end_the_trade() {
        let sides = [Some(TradeRole::Buyer), Some(TradeRole::Seller), None];
        for action in [
            Action::PurchaseCompleted,
            Action::HoldInvoicePaymentSettled,
            Action::Rate,
            Action::Canceled,
            Action::CooperativeCancelAccepted,
            Action::AdminCanceled,
        ] {
            for side in sides {
                assert!(ends_trade(&action, side), "{action:?} {side:?}");
            }
        }
        assert!(ends_trade(&Action::AdminSettled, Some(TradeRole::Seller)));
        assert!(!ends_trade(&Action::AdminSettled, Some(TradeRole::Buyer)));
        assert!(!ends_trade(&Action::AdminSettled, None));
        for action in [
            Action::HoldInvoicePaymentCanceled,
            Action::Released,
            Action::PaymentFailed,
            Action::AddInvoice,
            Action::BondSlashed,
        ] {
            for side in sides {
                assert!(!ends_trade(&action, side), "{action:?} {side:?}");
            }
        }
    }

    #[test]
    fn a_rate_follows_only_the_end_of_a_completed_trade() {
        assert!(rate_follows(&Action::PurchaseCompleted));
        assert!(rate_follows(&Action::HoldInvoicePaymentSettled));
        for action in [
            Action::Rate,
            Action::Canceled,
            Action::AdminSettled,
            Action::Released,
        ] {
            assert!(!rate_follows(&action), "{action:?}");
        }
    }

    #[test]
    fn watch_outcomes_conform_only_with_read_stages() {
        let schema = Value::Object(output_schema::<WatchData, ()>(&watch_rules()));
        let mut warnings = Warnings::new();
        let trade = Keys::generate().public_key();
        let peer = Keys::generate().public_key();
        let message = Message::new_order(Some(ORDER), None, None, Action::PurchaseCompleted, None);
        let record = MessageRecord::from_message(
            &id(1),
            NOW,
            &message,
            Some(3),
            &trade,
            None,
            &mut warnings,
        );
        let line = ChatRecord::new(&id(2), ChatSender::Counterparty, NOW, "paid", &mut warnings);
        let start = Cursor::start(NOW - 1_800);
        let seen = [Observed::mail(id(1), NOW), Observed::chat(id(2), NOW, NOW)];
        let cursor = start.advance(&seen, NOW);
        let full = WatchData::new(
            ORDER,
            Some(&peer),
            (vec![record], vec![line]),
            true,
            false,
            &cursor,
        );
        assert_eq!(full.order_id(), ORDER.to_string());
        assert_eq!(full.counterparty_pubkey(), Some(peer.to_hex().as_str()));
        assert_eq!((full.messages().len(), full.chat().len()), (1, 1));
        assert_eq!((full.ended(), full.truncated()), (true, false));
        assert_eq!(full.cursor(), cursor.to_string());
        let empty = || WatchData::new(ORDER, None, (Vec::new(), Vec::new()), false, true, &start);
        let warned = |codes: &[WarningCode]| {
            let mut warnings = Warnings::new();
            for code in codes {
                warnings.add(*code);
            }
            warnings
        };
        let ok = |data: WatchData, warnings| {
            serde_json::to_value(Envelope::<WatchData, ()>::ok(
                COMMAND,
                data,
                warnings,
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        let failed = |code| {
            serde_json::to_value(Envelope::<WatchData, ()>::error(
                Some(COMMAND),
                code,
                Warnings::new(),
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        let pilot = [
            WarningCode::InvalidEventTimestamp,
            WarningCode::CleanupIncomplete,
        ];
        let good = [
            ok(full, Warnings::new()),
            ok(empty(), Warnings::new()),
            ok(empty(), warned(&pilot)),
            failed(ErrorCode::InvalidArguments),
            failed(ErrorCode::OrderUnknown),
            failed(ErrorCode::ReadTimeout),
        ];
        for outcome in &good {
            assert!(conforms(&schema, &schema, outcome), "{outcome}");
        }

        let with = |base: &Value, pointer: &str, value: Value| {
            let mut changed = base.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            changed
        };
        let uncertain = serde_json::to_value(Envelope::<WatchData, ()>::uncertain(
            COMMAND,
            ErrorCode::ReadTimeout,
            None,
            Warnings::new(),
            ExecutionMeta::read(None),
        ))
        .unwrap();
        let bad = [
            with(&good[0], "/meta/snapshot", json!({})),
            with(&good[0], "/meta/persistence", json!("applied")),
            with(&good[1], "/data/cursor", json!("01")),
            with(
                &good[1],
                "/data/cursor",
                json!(format!("1:{}", "A".repeat(64))),
            ),
            with(&good[1], "/data/counterparty_pubkey", json!("npub1x")),
            failed(ErrorCode::CounterpartyUnknown),
            failed(ErrorCode::NodeRefused),
            uncertain,
            ok(empty(), warned(&[WarningCode::TradeKeysLimited])),
        ];
        for outcome in &bad {
            assert!(!conforms(&schema, &schema, outcome), "{outcome}");
        }
    }
}
