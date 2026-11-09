//! The records a watch returns (spec 020 D8): a node message or a chat
//! message, and the batches they come in.

use nostr_sdk::prelude::EventId;

use crate::msg::chat::ChatLine;
use crate::msg::inbox::Mail;
use crate::outcome::watch::Observed;

/// One record the watch accepted: a message of the node, or a chat message
/// opened from one of its envelopes.
#[derive(Clone, Debug)]
pub enum Seen {
    Mail(Box<Mail>),
    Chat {
        /// The envelope it came in; other envelopes can carry the same message.
        outer_id: EventId,
        /// The envelope's time, which relays filter on.
        created_at: u64,
        line: ChatLine,
    },
}

impl Seen {
    /// The record's id: a node message's event id, a chat message's inner
    /// `message_id`.
    pub fn id(&self) -> EventId {
        match self {
            Self::Mail(mail) => mail.event,
            Self::Chat { line, .. } => line.message_id,
        }
    }

    /// The event it came in.
    pub(super) fn outer_id(&self) -> EventId {
        match self {
            Self::Mail(mail) => mail.event,
            Self::Chat { outer_id, .. } => *outer_id,
        }
    }

    /// The outer `created_at`.
    pub fn created_at(&self) -> u64 {
        match self {
            Self::Mail(mail) => mail.created_at,
            Self::Chat { created_at, .. } => *created_at,
        }
    }

    pub(super) fn observed(&self) -> Observed {
        match self {
            Self::Mail(mail) => Observed::mail(mail.event, mail.created_at),
            Self::Chat {
                created_at, line, ..
            } => Observed::chat(line.message_id, *created_at, line.created_at),
        }
    }
}

/// New records, oldest first by outer time and id.
#[derive(Clone, Debug, Default)]
pub struct Batch {
    pub items: Vec<Seen>,
    /// Older records may be missing: since the last batch, a replay returned
    /// its limit of stored events and the oldest was new, or a live replay
    /// reached the notification capacity.
    pub truncated: bool,
}
