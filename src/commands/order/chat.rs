//! `order.chat`: the peer chat of an order the user holds, read and, with a
//! message, written once.
//!
//! The order's trade key comes from the store, the counterparty from
//! [`find_counterparty`] (the row, else the node's post-lock mail), and the
//! conversation from one bounded read. A message is sent only after that read
//! succeeded, so a failed read publishes nothing. Up to three short relay
//! sessions (lookup, read, send) and no database transaction across them.
//!
//! Stages (`Stages::Chat`): publication stays `not_applicable` until the send
//! step runs; the send step reports `published`, `not_attempted` or, when no
//! relay confirmed and one may have the message, `unknown` with an
//! `uncertain` outcome that still carries the conversation and the message.
//! Node acceptance and persistence are always `not_applicable`: the node never
//! sees a chat message, and the counterparty key the lookup may cache in the
//! row is not reported. Warnings of every step are kept, errors included.

use chrono::{DateTime, Utc};
use nostr_sdk::prelude::{Keys, PublicKey, Timestamp};
use uuid::Uuid;

use super::counterparty::find_counterparty;
use crate::msg::chat::{read_chat, send_chat, ChatLine, Conversation};
use crate::msg::{PublicReadConfig, ReadLimits};
use crate::outcome::chat::{ChatData, ChatInput, ChatRecord, ChatSender, COMMAND};
use crate::outcome::{DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Publication, Warnings};
use crate::store::{Store, StoreError};

/// The outcome of `order chat`; it never carries a snapshot.
pub type ChatOutcome = Envelope<ChatData, ()>;

/// What the steps before the send found, with the warnings of the lookup and
/// the read.
struct Read {
    order: Uuid,
    trade: Keys,
    peer: PublicKey,
    conversation: Conversation,
    warnings: Warnings,
}

impl Read {
    /// The conversation as records, with `sent` apart: a message read back
    /// with the id of the one just sent (the same text in the same second) is
    /// that message.
    fn data(&mut self, sent: Option<&ChatLine>) -> ChatData {
        let ours = self.trade.public_key();
        let warnings = &mut self.warnings;
        let mut record = |line: &ChatLine| {
            let from = ChatSender::of(&line.sender, &ours);
            ChatRecord::new(
                &line.message_id,
                from,
                line.created_at,
                &line.text,
                warnings,
            )
        };
        let messages = self
            .conversation
            .lines
            .iter()
            .filter(|line| sent.is_none_or(|sent| sent.message_id != line.message_id))
            .map(&mut record)
            .collect();
        let sent = sent.map(record);
        let read = (messages, self.conversation.truncated);
        ChatData::new(self.order, &self.peer, read, sent)
    }
}

/// Read the conversation of `input`'s order and send its message, if any.
///
/// `data.sent` is non-null exactly when the publication is `published` or
/// `unknown`, and `data.messages` never holds it.
pub async fn order_chat(
    input: &ChatInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &ReadLimits,
    now: DateTime<Utc>,
) -> ChatOutcome {
    let mut read = match read(input, config, store, limits, now).await {
        Ok(read) => read,
        Err((code, warnings)) => return unsent(code, warnings),
    };
    let Some(text) = input.message() else {
        let data = read.data(None);
        return Envelope::ok(COMMAND, data, read.warnings, ExecutionMeta::read(None));
    };
    let meta = |publication| ExecutionMeta::request(publication, DaemonAcceptance::NotApplicable);
    match send_chat(config, &read.trade, &read.peer, text, limits).await {
        Ok((line, sent)) => {
            read.warnings.merge(&sent);
            let data = read.data(Some(&line));
            Envelope::ok(COMMAND, data, read.warnings, meta(Publication::Published))
        }
        Err(failure) => {
            read.warnings.merge(failure.warnings());
            let (code, publication) = (failure.code(), meta(failure.publication()));
            let Some(line) = failure.line() else {
                return Envelope::error(Some(COMMAND), code, read.warnings, publication);
            };
            let data = read.data(Some(line));
            Envelope::uncertain(COMMAND, code, Some(data), read.warnings, publication)
        }
    }
}

/// The order's trade key, its counterparty and the conversation in the
/// window; a failure with its code and the warnings gathered so far.
async fn read(
    input: &ChatInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &ReadLimits,
    now: DateTime<Utc>,
) -> Result<Read, (ErrorCode, Warnings)> {
    let order = input.order();
    let trade = match store.order_keys(order).await {
        Ok(Some(keys)) => keys,
        Ok(None) => return Err((ErrorCode::OrderUnknown, Warnings::new())),
        Err(error) => return Err((store_code(error), Warnings::new())),
    };
    let seconds = u64::try_from(now.timestamp()).unwrap_or(0);
    let now = Timestamp::from(seconds);
    let found = find_counterparty(order, &trade, config, store, limits, now)
        .await
        .map_err(|failure| (failure.code(), failure.warnings().clone()))?;
    let (peer, mut warnings) = (found.key(), found.warnings().clone());
    let window = input.since_minutes().unsigned_abs() * 60;
    let since = Timestamp::from(seconds.saturating_sub(window));
    let conversation = match read_chat(config, &trade, &peer, since, now, limits).await {
        Ok(conversation) => conversation,
        Err(failure) => {
            warnings.merge(failure.warnings());
            return Err((failure.code(), warnings));
        }
    };
    warnings.merge(&conversation.warnings);
    Ok(Read {
        order,
        trade,
        peer,
        conversation,
        warnings,
    })
}

/// A failure before the send step: nothing was published.
fn unsent(code: ErrorCode, warnings: Warnings) -> ChatOutcome {
    Envelope::error(Some(COMMAND), code, warnings, ExecutionMeta::read(None))
}

fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}
