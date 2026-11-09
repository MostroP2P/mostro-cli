//! Waiting for the one reply that answers a request. Private to `msg`.
//!
//! The streams are armed before the request is published, so a reply that
//! arrives at once is already queued. An event decides the result only if the
//! existing correlation rule accepts it: a kind-14 from the node, addressed to
//! our key, not our own request, carrying our request identifier and an
//! allowed action. Everything else is ignored.

use std::pin::Pin;

use futures::stream::{select_all, Stream, StreamExt};
use mostro_core::prelude::{unwrap_incoming, Message};
use nostr_sdk::prelude::{
    Client, EventId, Keys, PublicKey, RelayMessage, RelayNotification, RelayStatus, RelayUrl,
    SubscriptionId,
};
use tokio::time::{timeout_at, Instant};

use crate::util::reply::{select_reply_event, ExpectedReply};

type Notifications = Pin<Box<dyn Stream<Item = (usize, RelayNotification)> + Send>>;

/// The notification streams of every configured relay, merged.
pub(super) struct Armed {
    notifications: Notifications,
    relays: usize,
}

/// What the next notification of any relay is.
pub(super) enum Received {
    /// A notification and the index of the relay that sent it.
    Notification(usize, RelayNotification),
    /// The deadline passed first.
    Timeout,
    /// Every relay's notification stream ended.
    Ended,
}

/// What the wait ended with.
pub(super) enum Waited {
    Reply(Box<Message>),
    Timeout,
    /// Every relay closed the subscription or went away.
    Lost,
}

/// What identifies the reply being waited for.
pub(super) struct Wait<'a> {
    pub identity: &'a Keys,
    pub node: PublicKey,
    pub sent: EventId,
    pub expected: ExpectedReply,
    pub subscription: &'a SubscriptionId,
    pub deadline: Instant,
}

/// Merge the notifications of `urls`; `None` if a relay is not in the pool.
pub(super) async fn arm(client: &Client, urls: &[RelayUrl]) -> Option<Armed> {
    let mut streams = Vec::with_capacity(urls.len());
    for (index, url) in urls.iter().enumerate() {
        let relay = client.relay(url).await.ok().flatten()?;
        streams.push(relay.notifications().map(move |n| (index, n)));
    }
    Some(Armed {
        notifications: Box::pin(select_all(streams)),
        relays: urls.len(),
    })
}

impl Armed {
    /// The next notification of any relay, waiting until `deadline` or, with
    /// none, for as long as it takes.
    pub(super) async fn recv(&mut self, deadline: Option<Instant>) -> Received {
        let next = match deadline {
            Some(deadline) => match timeout_at(deadline, self.notifications.next()).await {
                Err(_) => return Received::Timeout,
                Ok(next) => next,
            },
            None => self.notifications.next().await,
        };
        match next {
            Some((relay, notification)) => Received::Notification(relay, notification),
            None => Received::Ended,
        }
    }

    /// Wait for the answer until the deadline.
    pub(super) async fn wait(&mut self, wait: &Wait<'_>) -> Waited {
        let mut lost = vec![false; self.relays];
        loop {
            let (relay, notification) = match self.recv(Some(wait.deadline)).await {
                Received::Timeout => return Waited::Timeout,
                Received::Ended => return Waited::Lost,
                Received::Notification(relay, notification) => (relay, notification),
            };
            match notification {
                RelayNotification::Message { message } => match *message {
                    RelayMessage::Event {
                        subscription_id,
                        event,
                    } if *subscription_id == *wait.subscription => {
                        if let Some(reply) = answer(&event, wait).await {
                            return Waited::Reply(Box::new(reply));
                        }
                    }
                    RelayMessage::Closed {
                        subscription_id, ..
                    } if *subscription_id == *wait.subscription => lost[relay] = true,
                    _ => {}
                },
                RelayNotification::RelayStatus {
                    status:
                        RelayStatus::Disconnected | RelayStatus::Terminated | RelayStatus::Shutdown,
                } => lost[relay] = true,
                _ => {}
            }
            if lost.iter().all(|gone| *gone) {
                return Waited::Lost;
            }
        }
    }
}

/// The decoded message of `event` if it is the answer.
async fn answer(event: &nostr_sdk::prelude::Event, wait: &Wait<'_>) -> Option<Message> {
    let accepted = select_reply_event(
        event,
        wait.identity,
        wait.node,
        Some(wait.sent),
        Some(&wait.expected),
    )
    .await;
    if !accepted {
        return None;
    }
    unwrap_incoming(event, wait.identity)
        .await
        .ok()
        .flatten()
        .map(|unwrapped| unwrapped.message)
}
