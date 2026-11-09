//! The peer chat of an order: one bounded read of the conversation and the
//! send of one message (Mostro protocol chat, kind 14).
//!
//! Both sides derive the same pair of conversation keys from their trade keys
//! (`derive_chat_keys`): `K_conv`, which every message is encrypted to and
//! tagged with, and `K_sign`, which authors every envelope. The node is never
//! involved. The read asks every relay once for the envelopes `K_sign`
//! authored, through [`fetch_msg`] (so with its completion rules and caps), and
//! opens each one only through `unwrap_chat_message`, with our trade key and
//! the counterparty's as the only accepted writers. The send wraps one message,
//! publishes it once through the private publishing door and waits for one
//! relay to confirm it. Nothing is retried and nothing is written.

use std::collections::BTreeMap;

use mostro_core::prelude::{chat_filter, derive_chat_keys, unwrap_chat_message, wrap_chat_message};
use nostr_sdk::prelude::{Event, EventId, Keys, PublicKey, Relay, Timestamp};
use tokio::time::{timeout, Instant};

use super::send_msg::{publish, Unconfirmed};
use super::{fetch_msg, FetchFailure, PublicReadConfig, ReadLimits, Session};
use crate::outcome::messages::MAX_MESSAGES;
use crate::outcome::{ErrorCode, Publication, WarningCode, Warnings};

/// One message of the conversation, opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    /// The id of the signed message inside the envelope: the same however
    /// many relays or envelopes deliver it.
    pub message_id: EventId,
    /// Who signed the message: our trade key or the counterparty's.
    pub sender: PublicKey,
    /// When the message was signed, in seconds.
    pub created_at: u64,
    /// What was written, as written: untrusted.
    pub text: String,
}

/// The conversation in the window, oldest first.
#[derive(Debug)]
pub struct Conversation {
    pub lines: Vec<ChatLine>,
    /// Possibly not everything: a relay reached its limit or the cap did.
    pub truncated: bool,
    pub warnings: Warnings,
}

/// Why a message was not confirmed, and what is known about it.
#[derive(Clone, Debug)]
pub struct SendFailure {
    code: ErrorCode,
    publication: Publication,
    line: Option<ChatLine>,
    warnings: Warnings,
}

impl SendFailure {
    fn unsent(code: ErrorCode) -> Self {
        Self {
            code,
            publication: Publication::NotAttempted,
            line: None,
            warnings: Warnings::new(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// `not_attempted` when no relay can have the message, `unknown` when one
    /// may.
    pub fn publication(&self) -> Publication {
        self.publication
    }

    /// The message that may have been published: only when the publication is
    /// `unknown`.
    pub fn line(&self) -> Option<&ChatLine> {
        self.line.as_ref()
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Read the conversation of `trade` with `peer` written from `since` on, as
/// seen at `now`: both sides, oldest first, at most [`MAX_MESSAGES`].
pub async fn read_chat(
    config: &PublicReadConfig,
    trade: &Keys,
    peer: &PublicKey,
    since: Timestamp,
    now: Timestamp,
    limits: &ReadLimits,
) -> Result<Conversation, FetchFailure> {
    let (conv, sign) = derive_chat_keys(trade, peer)
        .map_err(|_| FetchFailure::before_query(ErrorCode::InternalError))?;
    let filter = chat_filter(sign.public_key())
        .since(since)
        .limit(MAX_MESSAGES);
    let fetched = fetch_msg(config, filter, limits).await?;
    let writers = [trade.public_key(), *peer];
    let (lines, capped) = open_lines(
        fetched.events(),
        (&conv, &sign.public_key()),
        &writers,
        since,
        now,
    );
    Ok(Conversation {
        lines,
        truncated: capped || fetched.events().len() >= MAX_MESSAGES,
        warnings: fetched.warnings().clone(),
    })
}

/// The messages of one conversation in `events`, oldest first by time and
/// id, and whether the cap left some out.
///
/// An envelope counts only if `unwrap_chat_message` accepts it as seen at
/// `now`: authored by `K_sign`, tagged with `K_conv` alone, no more than 60
/// seconds ahead of `now`, of bounded size, with valid signatures, and written
/// inside by one of `writers`. A message written before `since` is out; one
/// delivered in several envelopes counts once; past [`MAX_MESSAGES`] only
/// the newest are kept.
pub(crate) fn open_lines<'a>(
    events: impl IntoIterator<Item = &'a Event>,
    (conv, sign): (&Keys, &PublicKey),
    writers: &[PublicKey],
    since: Timestamp,
    now: Timestamp,
) -> (Vec<ChatLine>, bool) {
    let mut unique: BTreeMap<EventId, ChatLine> = BTreeMap::new();
    for event in events {
        let Ok(chat) = unwrap_chat_message(conv, sign, writers, event, now) else {
            continue;
        };
        if chat.created_at < since {
            continue;
        }
        unique.entry(chat.inner_event_id).or_insert(ChatLine {
            message_id: chat.inner_event_id,
            sender: chat.sender,
            created_at: chat.created_at.as_secs(),
            text: chat.content,
        });
    }
    let mut lines: Vec<ChatLine> = unique.into_values().collect();
    lines.sort_by_key(|line| (line.created_at, line.message_id));
    let capped = lines.len() > MAX_MESSAGES;
    if capped {
        lines.drain(..lines.len() - MAX_MESSAGES);
    }
    (lines, capped)
}

/// Publish `text` from `trade` to the conversation with `peer`, once: the send
/// step of `order chat`.
///
/// The message is wrapped and opened again before any relay is contacted, so
/// the line returned is the one the counterparty will read. Every configured
/// relay is connected and sent it once; `Ok` as soon as one confirms, after
/// the others have answered or the read budget is spent. A wrap that fails is
/// `internal_error`, a connection that fails keeps its code, and every relay
/// refusing it is `request_rejected`: nothing was published. No relay
/// confirming while one failed without a clear answer is `request_rejected`
/// with publication `unknown` and the line, since a relay may have it.
pub async fn send_chat(
    config: &PublicReadConfig,
    trade: &Keys,
    peer: &PublicKey,
    text: &str,
    limits: &ReadLimits,
) -> Result<(ChatLine, Warnings), SendFailure> {
    let (event, line) = wrap(trade, peer, text)
        .await
        .ok_or_else(|| SendFailure::unsent(ErrorCode::InternalError))?;
    let session = Session::connect(config, limits)
        .await
        .map_err(SendFailure::unsent)?;

    let deadline = Instant::now() + limits.read;
    let relays: Vec<Relay> = session.client.relays().await.into_values().collect();
    let mut publishing = publish(&relays, &event);
    let result = match publishing.confirmed(deadline).await {
        Ok(()) => {
            publishing.settle(deadline).await;
            Ok(())
        }
        Err(Unconfirmed::Rejected) => Err(SendFailure::unsent(ErrorCode::RequestRejected)),
        Err(Unconfirmed::Ambiguous) => Err(SendFailure {
            publication: Publication::Unknown,
            line: Some(line.clone()),
            ..SendFailure::unsent(ErrorCode::RequestRejected)
        }),
    };
    drop(publishing);

    let mut warnings = Warnings::new();
    if timeout(limits.cleanup, session.client.shutdown())
        .await
        .is_err()
    {
        warnings.add(WarningCode::CleanupIncomplete);
    }
    match result {
        Ok(()) => Ok((line, warnings)),
        Err(mut failure) => {
            failure.warnings.merge(&warnings);
            Err(failure)
        }
    }
}

/// The envelope of `text` and the line it opens to, or `None` if either step
/// fails.
async fn wrap(trade: &Keys, peer: &PublicKey, text: &str) -> Option<(Event, ChatLine)> {
    let (conv, sign) = derive_chat_keys(trade, peer).ok()?;
    let event = wrap_chat_message(trade, &conv, &sign, text).await.ok()?;
    let writers = [trade.public_key()];
    let (mut lines, _) = open_lines(
        [&event],
        (&conv, &sign.public_key()),
        &writers,
        Timestamp::from(0),
        Timestamp::now(),
    );
    Some((event, lines.pop()?))
}

#[cfg(test)]
mod tests {
    use mostro_core::prelude::CHAT_MAX_CLOCK_SKEW_SECS;
    use nostr_sdk::prelude::{nip44, EventBuilder, FinalizeEvent, Kind, Tag};

    use super::*;

    /// The two sides of one conversation and its keys.
    struct Pair {
        me: Keys,
        them: Keys,
        conv: Keys,
        sign: Keys,
    }

    impl Pair {
        fn new() -> Self {
            let (me, them) = (Keys::generate(), Keys::generate());
            let (conv, sign) = derive_chat_keys(&me, &them.public_key()).unwrap();
            Self {
                me,
                them,
                conv,
                sign,
            }
        }

        /// A note `writer` signed at `at`.
        fn note(writer: &Keys, text: &str, at: u64) -> Event {
            EventBuilder::new(Kind::TextNote, text)
                .custom_created_at(Timestamp::from(at))
                .finalize(writer)
                .unwrap()
        }

        /// `inner` in an envelope `author` signed at `at`, as
        /// `wrap_chat_message` builds it but at any time.
        fn envelope(&self, inner: &Event, author: &Keys, at: u64) -> Event {
            let content = nip44::encrypt(
                self.conv.secret_key(),
                &self.conv.public_key(),
                inner.as_json(),
                nip44::Version::V2,
            )
            .unwrap();
            EventBuilder::new(Kind::PrivateDirectMessage, content)
                .tags([Tag::public_key(self.conv.public_key())])
                .custom_created_at(Timestamp::from(at))
                .finalize(author)
                .unwrap()
        }

        /// A message `writer` wrote at `at`, in its proper envelope.
        fn message(&self, writer: &Keys, text: &str, at: u64) -> Event {
            self.envelope(&Self::note(writer, text, at), &self.sign, at)
        }

        fn open(&self, events: &[Event], since: u64, now: u64) -> (Vec<ChatLine>, bool) {
            open_lines(
                events,
                (&self.conv, &self.sign.public_key()),
                &[self.me.public_key(), self.them.public_key()],
                Timestamp::from(since),
                Timestamp::from(now),
            )
        }
    }

    fn texts(lines: &[ChatLine]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[tokio::test]
    async fn both_sides_come_back_oldest_first_once() {
        let pair = Pair::new();
        let stranger = Keys::generate();
        let theirs = Pair::note(&pair.them, "got it", 1_000);
        let real = wrap_chat_message(&pair.me, &pair.conv, &pair.sign, "sent")
            .await
            .unwrap();
        let now = real.created_at.as_secs();
        let events = [
            pair.envelope(&theirs, &pair.sign, 1_000),
            pair.envelope(&theirs, &pair.sign, 1_030),
            pair.message(&pair.me, "fiat sent", 900),
            real.clone(),
            pair.envelope(&Pair::note(&pair.them, "forged", 950), &stranger, 950),
            pair.message(&stranger, "intruder", 960),
        ];

        let (lines, capped) = pair.open(&events, 0, now);

        assert_eq!(texts(&lines), ["fiat sent", "got it", "sent"]);
        assert!(!capped);
        assert_eq!(lines[1].message_id, theirs.id, "the inner id, once");
        assert_eq!(
            (lines[0].sender, lines[1].sender, lines[2].sender),
            (
                pair.me.public_key(),
                pair.them.public_key(),
                pair.me.public_key()
            )
        );
        assert_eq!(lines[0].created_at, 900);
    }

    #[test]
    fn the_window_and_the_cap_keep_the_newest_200() {
        let pair = Pair::new();
        let since = 10_000;
        let mut events: Vec<Event> = (0..3)
            .map(|n| pair.message(&pair.them, "too old", since - 10 + n))
            .collect();
        let at = |n: usize| since + u64::try_from(n).unwrap();
        events.extend((0..MAX_MESSAGES + 2).map(|n| pair.message(&pair.me, &n.to_string(), at(n))));
        let now = at(MAX_MESSAGES + 2);

        let (lines, capped) = pair.open(&events, since, now);

        assert!(capped);
        assert_eq!(lines.len(), MAX_MESSAGES);
        assert_eq!(lines[0].text, "2", "the two oldest in the window are out");
        assert_eq!(lines[MAX_MESSAGES - 1].text, (MAX_MESSAGES + 1).to_string());
        let (exact, capped) = pair.open(&events[3..MAX_MESSAGES + 3], since, now);
        assert_eq!((exact.len(), capped), (MAX_MESSAGES, false));
        assert_eq!(pair.open(&events[..3], since, now).0, Vec::new());
    }

    #[test]
    fn a_message_59_seconds_ahead_is_kept_and_61_is_dropped() {
        let pair = Pair::new();
        let at = 50_000;
        let ahead = [pair.message(&pair.them, "early", at)];
        assert_eq!(CHAT_MAX_CLOCK_SKEW_SECS, 60);

        assert_eq!(texts(&pair.open(&ahead, 0, at - 59).0), ["early"]);
        assert!(pair.open(&ahead, 0, at - 61).0.is_empty());
    }
}
