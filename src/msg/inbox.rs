//! The user's mailbox: the messages the node sent to a set of trade keys.
//!
//! One bounded read of every configured relay for kind-14 events authored by
//! the node and addressed to any of the keys, through [`fetch_msg`] (so with
//! its completion rules and caps), then each event is opened with the key it
//! names. The author is checked here, not left to the relay: an event signed by
//! anyone else is dropped even if it opens. Nothing is written and nothing is
//! retried.

use std::collections::BTreeSet;

use mostro_core::prelude::{unwrap_incoming, Message};
use nostr_sdk::prelude::{Event, EventId, Filter, Keys, Kind, PublicKey, Timestamp};

use super::orders::PER_RELAY_LIMIT;
use super::{fetch_msg, FetchFailure, PublicReadConfig, ReadLimits};
use crate::outcome::messages::MAX_MESSAGES;
use crate::outcome::order::Snapshot;
use crate::outcome::Warnings;

/// One message of the node, opened.
#[derive(Clone, Debug)]
pub struct Mail {
    pub event: EventId,
    pub created_at: u64,
    pub message: Message,
    /// The index of the key it was addressed to, when known.
    pub index: Option<i64>,
    pub key: PublicKey,
}

/// What the read found, oldest first, and the evidence of the read.
#[derive(Debug)]
pub struct Inbox {
    pub mail: Vec<Mail>,
    /// Possibly not everything: a relay reached its limit or the record cap did.
    pub truncated: bool,
    pub snapshot: Snapshot,
    pub warnings: Warnings,
}

/// Read the messages of the node addressed to `keys` since `since`.
///
/// With no key nothing is asked: a filter without a recipient would match every
/// user's mail.
pub async fn read_inbox(
    config: &PublicReadConfig,
    keys: &[(Option<i64>, Keys)],
    since: Timestamp,
    now: Timestamp,
    limits: &ReadLimits,
) -> Result<Inbox, FetchFailure> {
    if keys.is_empty() {
        return Ok(Inbox {
            mail: Vec::new(),
            truncated: false,
            snapshot: Snapshot::new(
                since,
                now,
                PER_RELAY_LIMIT,
                limits.max_unique_events,
                config.relays().len(),
                0,
                0,
            ),
            warnings: Warnings::new(),
        });
    }
    let node = config.mostro_pubkey();
    let filter = mail_filter(node, keys, since);
    let fetched = fetch_msg(config, filter, limits).await?;

    let mut mail = open_events(fetched.events(), keys, node, since).await;
    // Oldest first, ties by event id; the newest are kept when over the cap.
    mail.sort_by_key(|mail| (mail.created_at, mail.event));
    let capped = mail.len() > MAX_MESSAGES;
    if capped {
        mail.drain(..mail.len() - MAX_MESSAGES);
    }
    Ok(Inbox {
        mail,
        truncated: capped || fetched.events().len() >= PER_RELAY_LIMIT,
        snapshot: fetched.snapshot().clone(),
        warnings: fetched.warnings().clone(),
    })
}

/// The node's kind-14 mail to any of `keys` since `since`, at most
/// [`PER_RELAY_LIMIT`] stored events per relay.
pub(super) fn mail_filter(
    node: PublicKey,
    keys: &[(Option<i64>, Keys)],
    since: Timestamp,
) -> Filter {
    Filter::new()
        .kind(Kind::PrivateDirectMessage)
        .author(node)
        .pubkeys(keys.iter().map(|(_, key)| key.public_key()))
        .since(since)
        .limit(PER_RELAY_LIMIT)
}

/// Open the events that are the node's mail for one of `keys` since `since`.
/// Everything is checked here, whatever the relay was asked: the author, the
/// kind, the time, the recipient tag and that the message opens with that key
/// and was sent by the node.
pub(crate) async fn open_events(
    events: &BTreeSet<Event>,
    keys: &[(Option<i64>, Keys)],
    node: PublicKey,
    since: Timestamp,
) -> Vec<Mail> {
    let mut mail = Vec::new();
    for event in events {
        if event.kind != Kind::PrivateDirectMessage
            || event.pubkey != node
            || event.created_at < since
        {
            continue;
        }
        let tagged: Vec<PublicKey> = event.tags.public_keys().collect();
        let Some((index, key)) = keys
            .iter()
            .find(|(_, key)| tagged.contains(&key.public_key()))
        else {
            continue;
        };
        if let Ok(Some(opened)) = unwrap_incoming(event, key).await {
            if opened.sender == node {
                mail.push(Mail {
                    event: event.id,
                    created_at: event.created_at.as_secs(),
                    message: opened.message,
                    index: *index,
                    key: key.public_key(),
                });
            }
        }
    }
    mail
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::{wrap_message_with, Action, Transport, WrapOptions};
    use nostr_sdk::prelude::{Event, EventBuilder, FinalizeEvent, Tag};
    use uuid::Uuid;

    async fn event(author: &Keys, to: &Keys, at: u64) -> Event {
        let message = Message::new_order(
            Some(Uuid::from_u128(1)),
            Some(1),
            None,
            Action::WaitingSellerToPay,
            None,
        );
        let wrapped = wrap_message_with(
            Transport::Nip44Direct,
            &message,
            author,
            author,
            to.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();
        EventBuilder::new(Kind::PrivateDirectMessage, wrapped.content.clone())
            .tags(wrapped.tags.clone())
            .custom_created_at(Timestamp::from(at))
            .finalize(author)
            .unwrap()
    }

    #[tokio::test]
    async fn only_the_nodes_mail_for_these_keys_since_the_bound_is_opened() {
        let (node, impostor, mine, other) = (
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
        );
        let keys = [(Some(1), mine.clone())];
        let good = event(&node, &mine, 1_000).await;
        let events: BTreeSet<Event> = [
            good.clone(),
            event(&impostor, &mine, 1_000).await,
            event(&node, &other, 1_000).await,
            event(&node, &mine, 500).await,
            EventBuilder::new(Kind::TextNote, "x")
                .tags([Tag::public_key(mine.public_key())])
                .custom_created_at(Timestamp::from(1_000))
                .finalize(&node)
                .unwrap(),
        ]
        .into_iter()
        .collect();

        let mail = open_events(&events, &keys, node.public_key(), Timestamp::from(900)).await;

        assert_eq!(
            mail.len(),
            1,
            "an impostor, a stranger's mail, old mail and a note are out"
        );
        assert_eq!((mail[0].event, mail[0].index), (good.id, Some(1)));
        assert_eq!(mail[0].key, mine.public_key());
    }

    #[tokio::test]
    async fn the_index_follows_the_key_the_event_names() {
        let (node, a, b) = (Keys::generate(), Keys::generate(), Keys::generate());
        let keys = [(Some(7), a.clone()), (None, b.clone())];
        let events: BTreeSet<Event> =
            [event(&node, &a, 1_000).await, event(&node, &b, 1_001).await]
                .into_iter()
                .collect();

        let mut mail = open_events(&events, &keys, node.public_key(), Timestamp::from(0)).await;

        mail.sort_by_key(|mail| mail.created_at);
        assert_eq!(
            mail.iter().map(|mail| mail.index).collect::<Vec<_>>(),
            [Some(7), None]
        );
    }
}
