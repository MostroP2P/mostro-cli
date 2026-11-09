//! `order.chat` input and data: the peer chat of an order the user holds, as
//! typed records.
//!
//! Chat text is untrusted: the counterparty writes whatever it likes. It is
//! data, bounded here and escaped by the human renderer, never part of an
//! error. A record says who wrote it (`me` or `counterparty`), never a key, and
//! no trade secret is serialized.

use std::fmt;

use mostro_core::prelude::CHAT_DEFAULT_LOOKBACK_SECS;
use nostr_sdk::prelude::{EventId, PublicKey};
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::messages::{bounded, TEXT_BYTES};
use super::order::rfc3339_utc;
use super::rules::OutcomeRules;
use super::{schema, ErrorCode, WarningCode, Warnings};

/// Canonical name of `order chat`.
pub const COMMAND: &str = "order.chat";
/// The longest window: the seven days a chat client reads by default.
pub const MAX_SINCE_MINUTES: i64 = (CHAT_DEFAULT_LOOKBACK_SECS / 60) as i64;
/// The window read by default: the whole chat window.
pub const DEFAULT_SINCE_MINUTES: i64 = MAX_SINCE_MINUTES;
/// The longest message sent, after trimming: as long as a received text may
/// be, so a message we send is never cut when it is read back.
pub const MAX_MESSAGE_BYTES: usize = TEXT_BYTES;

/// Codes that fail `order.chat`.
pub const ERRORS: [ErrorCode; 14] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
    ErrorCode::StoreUnreadable,
    ErrorCode::OrderUnknown,
    ErrorCode::CounterpartyUnknown,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::ReadTimeout,
    ErrorCode::ReadLimitExceeded,
    ErrorCode::RequestRejected,
    ErrorCode::InternalError,
];

/// Codes that leave it `uncertain`: no relay confirmed the message and one
/// may have it.
pub const UNCERTAIN: [ErrorCode; 1] = [ErrorCode::RequestRejected];

/// What `order.chat`'s outcomes may contain.
pub fn chat_rules() -> OutcomeRules<'static> {
    OutcomeRules::chat(COMMAND, &ERRORS, &UNCERTAIN)
}

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidChat {
    Message,
    Since,
}

impl fmt::Display for InvalidChat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Message => "invalid value for --message",
            Self::Since => "invalid value for --since",
        })
    }
}

impl std::error::Error for InvalidChat {}

/// A validated `order chat`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatInput {
    order: Uuid,
    message: Option<String>,
    since_minutes: i64,
}

impl ChatInput {
    /// The message is trimmed and must then hold 1 to [`MAX_MESSAGE_BYTES`]
    /// bytes; the window is 1 to [`MAX_SINCE_MINUTES`] minutes.
    pub fn validate(
        order: Uuid,
        message: Option<&str>,
        since: Option<i64>,
    ) -> Result<Self, InvalidChat> {
        let since_minutes = since.unwrap_or(DEFAULT_SINCE_MINUTES);
        if !(1..=MAX_SINCE_MINUTES).contains(&since_minutes) {
            return Err(InvalidChat::Since);
        }
        let message = message.map(str::trim);
        if message.is_some_and(|text| text.is_empty() || text.len() > MAX_MESSAGE_BYTES) {
            return Err(InvalidChat::Message);
        }
        Ok(Self {
            order,
            message: message.map(str::to_owned),
            since_minutes,
        })
    }

    pub fn order(&self) -> Uuid {
        self.order
    }

    /// The trimmed message to send, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn since_minutes(&self) -> i64 {
        self.since_minutes
    }
}

/// Who wrote a chat message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChatSender {
    /// The order's trade key: the user.
    Me,
    /// The counterparty's trade key.
    Counterparty,
}

impl ChatSender {
    /// `Me` when `sender` is `ours`, the counterparty otherwise (the chat read
    /// accepts no third signer).
    pub fn of(sender: &PublicKey, ours: &PublicKey) -> Self {
        if sender == ours {
            Self::Me
        } else {
            Self::Counterparty
        }
    }
}

/// One message of the conversation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChatRecord {
    /// The id of the signed message inside the envelope: the same on every
    /// relay that delivers it.
    #[schemars(schema_with = "schema::pubkey_hex")]
    message_id: String,
    from: ChatSender,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    created_at: Option<String>,
    #[schemars(schema_with = "schema::decimal")]
    created_at_unix: String,
    /// What was written: untrusted data, at most 4096 bytes, cut on a
    /// character boundary.
    text: String,
}

impl ChatRecord {
    /// The message `message_id`, written by `from` at `created_at` seconds. A
    /// time outside four-digit UTC years adds the aggregate
    /// `invalid_event_timestamp` warning and leaves the RFC 3339 field null.
    pub fn new(
        message_id: &EventId,
        from: ChatSender,
        created_at: u64,
        text: &str,
        warnings: &mut Warnings,
    ) -> Self {
        let unix = i64::try_from(created_at).unwrap_or(i64::MAX);
        let created = rfc3339_utc(unix);
        if created.is_none() {
            warnings.add(WarningCode::InvalidEventTimestamp);
        }
        Self {
            message_id: message_id.to_hex(),
            from,
            created_at: created,
            created_at_unix: unix.to_string(),
            text: bounded(text, TEXT_BYTES),
        }
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    pub fn from(&self) -> ChatSender {
        self.from
    }

    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    pub fn created_at_unix(&self) -> &str {
        &self.created_at_unix
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// `order.chat` success data, and the data of an `uncertain` send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChatData {
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    /// The counterparty's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    counterparty_pubkey: String,
    /// The conversation in the window, oldest first, at most 200; never the
    /// message sent on this run.
    messages: Vec<ChatRecord>,
    /// The conversation may not be complete: a relay or the cap of 200 may have
    /// left out older messages.
    truncated: bool,
    /// The message sent on this run: non-null exactly when the publication is
    /// `published` or `unknown`.
    sent: Option<ChatRecord>,
}

impl ChatData {
    pub fn new(
        order: Uuid,
        counterparty: &PublicKey,
        (messages, truncated): (Vec<ChatRecord>, bool),
        sent: Option<ChatRecord>,
    ) -> Self {
        Self {
            order_id: order.to_string(),
            counterparty_pubkey: counterparty.to_hex(),
            messages,
            truncated,
            sent,
        }
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn counterparty_pubkey(&self) -> &str {
        &self.counterparty_pubkey
    }

    pub fn messages(&self) -> &[ChatRecord] {
        &self.messages
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn sent(&self) -> Option<&ChatRecord> {
        self.sent.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::schema::output_schema;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::{DaemonAcceptance, Envelope, ExecutionMeta, Publication};
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};

    const ORDER: Uuid = Uuid::from_u128(0x0c4a7);
    const AT: u64 = 1_792_108_800;

    fn record(from: ChatSender, text: &str) -> ChatRecord {
        ChatRecord::new(
            &EventId::from_byte_array([7; 32]),
            from,
            AT,
            text,
            &mut Warnings::new(),
        )
    }

    fn data(sent: Option<ChatRecord>) -> ChatData {
        let messages = vec![
            record(ChatSender::Me, "hello"),
            record(ChatSender::Counterparty, "hi"),
        ];
        ChatData::new(
            ORDER,
            &Keys::generate().public_key(),
            (messages, false),
            sent,
        )
    }

    fn schema() -> Value {
        Value::Object(output_schema::<ChatData, ()>(&chat_rules()))
    }

    fn meta(publication: Publication) -> ExecutionMeta<()> {
        ExecutionMeta::request(publication, DaemonAcceptance::NotApplicable)
    }

    #[test]
    fn the_window_defaults_to_seven_days_and_is_bounded() {
        let input = ChatInput::validate(ORDER, None, None).unwrap();
        assert_eq!(
            (input.order(), input.message(), input.since_minutes()),
            (ORDER, None, 10_080)
        );
        for good in [1, 10_080] {
            let input = ChatInput::validate(ORDER, None, Some(good));
            assert_eq!(input.map(|input| input.since_minutes()), Ok(good));
        }
        for bad in [0, -1, 10_081, i64::MIN, i64::MAX] {
            let error = ChatInput::validate(ORDER, None, Some(bad)).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --since", "{bad}");
        }
    }

    #[test]
    fn the_message_is_trimmed_and_must_be_1_to_4096_bytes() {
        let input = ChatInput::validate(ORDER, Some("  paid, check  \n"), None).unwrap();
        assert_eq!(input.message(), Some("paid, check"));
        let longest = "é".repeat(MAX_MESSAGE_BYTES / 2);
        let padded = format!(" {longest}\t");
        let input = ChatInput::validate(ORDER, Some(&padded), Some(5)).unwrap();
        assert_eq!(
            input.message(),
            Some(longest.as_str()),
            "4096 bytes trimmed"
        );
        let secret = "secret-sentinel";
        for bad in [
            String::new(),
            " \n\t ".to_string(),
            format!("{secret}{}", "x".repeat(MAX_MESSAGE_BYTES)),
        ] {
            let error = ChatInput::validate(ORDER, Some(&bad), None).unwrap_err();
            let shown = format!("{error} {error:?}");
            assert_eq!(error.to_string(), "invalid value for --message");
            assert!(!shown.contains(secret), "{shown}");
        }
    }

    #[test]
    fn a_record_bounds_untrusted_text_on_a_character_boundary() {
        let mut warnings = Warnings::new();
        let long = "é".repeat(TEXT_BYTES);
        let id = EventId::from_byte_array([9; 32]);

        let kept = ChatRecord::new(&id, ChatSender::Counterparty, AT, &long, &mut warnings);

        assert_eq!(kept.text().len(), TEXT_BYTES);
        assert!(long.starts_with(kept.text()));
        assert_eq!(kept.message_id(), id.to_hex());
        assert_eq!(kept.created_at(), Some("2026-10-16T00:00:00Z"));
        assert_eq!(kept.created_at_unix(), AT.to_string());
        assert!(warnings.is_empty());
        let odd = ChatRecord::new(&id, ChatSender::Me, u64::MAX, "x", &mut warnings);
        ChatRecord::new(&id, ChatSender::Me, u64::MAX, "y", &mut warnings);
        assert_eq!(odd.created_at(), None);
        assert_eq!(odd.created_at_unix(), i64::MAX.to_string());
        assert!(warnings.contains(WarningCode::InvalidEventTimestamp));
        assert_eq!(
            serde_json::to_value(&warnings).unwrap()[1],
            Value::Null,
            "one"
        );
        let me = Keys::generate().public_key();
        let them = Keys::generate().public_key();
        assert_eq!(ChatSender::of(&me, &me), ChatSender::Me);
        assert_eq!(ChatSender::of(&them, &me), ChatSender::Counterparty);
    }

    #[test]
    fn chat_outcomes_conform_only_with_chat_stages() {
        let schema = schema();
        let sent = || Some(record(ChatSender::Me, "sent"));
        let ok = |data: ChatData, meta| {
            serde_json::to_value(Envelope::ok(COMMAND, data, Warnings::new(), meta)).unwrap()
        };
        let failed = |code, meta| {
            serde_json::to_value(Envelope::<ChatData, ()>::error(
                Some(COMMAND),
                code,
                Warnings::new(),
                meta,
            ))
            .unwrap()
        };
        let uncertain = serde_json::to_value(Envelope::uncertain(
            COMMAND,
            ErrorCode::RequestRejected,
            Some(data(sent())),
            Warnings::new(),
            meta(Publication::Unknown),
        ))
        .unwrap();
        let good = [
            ok(data(None), ExecutionMeta::read(None)),
            ok(data(sent()), meta(Publication::Published)),
            failed(ErrorCode::CounterpartyUnknown, ExecutionMeta::read(None)),
            failed(ErrorCode::InvalidArguments, ExecutionMeta::read(None)),
            failed(ErrorCode::RequestRejected, meta(Publication::NotAttempted)),
            failed(ErrorCode::InternalError, meta(Publication::NotAttempted)),
            failed(
                ErrorCode::RelayConnectionFailed,
                meta(Publication::NotAttempted),
            ),
            uncertain.clone(),
        ];
        for outcome in &good {
            assert!(conforms(&schema, &schema, outcome), "{outcome}");
        }

        let with = |base: &Value, pointer: &str, value: Value| {
            let mut changed = base.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            changed
        };
        let bad = [
            with(&good[1], "/meta/daemon_acceptance", json!("accepted")),
            with(&good[1], "/meta/persistence", json!("applied")),
            with(&good[0], "/meta/snapshot", json!({})),
            with(&good[0], "/data/messages/0/from", json!("solver")),
            with(&good[0], "/meta/publication", json!("not_attempted")),
            with(&good[2], "/meta/publication", json!("published")),
            with(&good[4], "/meta/publication", json!("unknown")),
            with(&uncertain, "/meta/publication", json!("published")),
            with(&uncertain, "/error/code", json!("read_timeout")),
            failed(ErrorCode::NodeRefused, ExecutionMeta::read(None)),
        ];
        for outcome in &bad {
            assert!(!conforms(&schema, &schema, outcome), "{outcome}");
        }
    }
}
