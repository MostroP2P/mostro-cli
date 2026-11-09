//! The live watch of one order (spec 020): the node's mail to the order's
//! trade key and, once the counterparty is known, the order's peer chat. It
//! only subscribes; nothing is published.
//!
//! One connection to every configured relay, notifications armed before any
//! REQ, and one REQ per relay and filter (`ReqTarget::single`), so a relay that
//! takes one filter per REQ serves both. The opening replay is strict, as for
//! the reads: every relay must take its REQs and send EOSE within the read
//! budget. After it no relay event ends the watch. A relay that goes away is
//! retried at a fixed interval; when it is back the SDK sends again every
//! subscription it still holds, and the watch sends again, under a new id,
//! each one it no longer holds (after a CLOSED, or never sent while it was
//! away). A relay that refuses while it stays connected is not asked again
//! until it reconnects. New records are held back while any relay replays,
//! a reconnect included (`state.rs`).
//!
//! Each event counts toward its replay's stored events, then is routed by its
//! author: the node's through `open_events` (unless already seen), the
//! conversation key's through `open_lines`, one event at a time. Anything else,
//! or anything that does not open, is dropped and never returned.

use std::collections::{BTreeSet, HashSet};
use std::future::pending;
use std::time::Duration;

use mostro_core::prelude::{chat_filter, derive_chat_keys, CHAT_MAX_CLOCK_SKEW_SECS};
use nostr_sdk::prelude::{
    Event, Filter, Keys, PublicKey, RelayMessage, RelayNotification, RelayStatus, RelayUrl,
    ReqTarget, SubscriptionId, Timestamp,
};
use tokio::time::{sleep_until, timeout, Instant};

use super::chat::open_lines;
use super::inbox::{mail_filter, open_events};
use super::orders::PER_RELAY_LIMIT;
use super::reply::{arm, Armed, Received};
use super::{PublicReadConfig, Session};
use crate::outcome::watch::{Cursor, Observed};
use crate::outcome::{ErrorCode, WarningCode, Warnings};

mod limits;
mod records;
mod state;

pub use limits::{WatchFailure, WatchLimits};
pub use records::{Batch, Seen};
use state::{Feed, State};

/// The conversation once the counterparty is known.
struct ChatKeys {
    conv: Keys,
    sign: PublicKey,
    writers: [PublicKey; 2],
    filter: Filter,
}

/// A watch on one order. Close it with [`Watch::close`].
pub struct Watch {
    session: Session,
    armed: Armed,
    relays: Vec<RelayUrl>,
    node: PublicKey,
    trade: [(Option<i64>, Keys); 1],
    since: Timestamp,
    mail: Filter,
    chat: Option<ChatKeys>,
    state: State,
    cleanup: Duration,
}

impl Watch {
    /// Connect every relay and replay the node's mail to `trade` (with its
    /// index) since the time of `start`, and the chat with `peer` when it is
    /// known; the ids `start` lists are observed but not returned again. The
    /// replayed records come back from the first [`Watch::next`]. A relay that
    /// does not connect is `relay_connection_failed`; one that refuses a REQ,
    /// closes it or goes away before its EOSE, `relay_read_failed`; one
    /// without EOSE within the read budget, `read_timeout`.
    pub async fn open(
        config: &PublicReadConfig,
        (index, trade): (Option<i64>, &Keys),
        peer: Option<&PublicKey>,
        start: &Cursor,
        limits: &WatchLimits,
    ) -> Result<Self, WatchFailure> {
        let session = Session::live(config, &limits.read, limits.retry)
            .await
            .map_err(|code| WatchFailure {
                code,
                warnings: Warnings::new(),
            })?;
        let cleanup = limits.read.cleanup;
        let Some(armed) = arm(&session.client, config.relays()).await else {
            let warnings = shut_down(&session, cleanup).await;
            let code = ErrorCode::InternalError;
            return Err(WatchFailure { code, warnings });
        };
        let (node, since) = (config.mostro_pubkey(), Timestamp::from(start.since()));
        let trade = [(index, trade.clone())];
        let mut watch = Self {
            session,
            armed,
            relays: config.relays().to_vec(),
            node,
            mail: mail_filter(node, &trade, since),
            trade,
            since,
            chat: None,
            state: State::new(config.relays().len(), start.ids(), &limits.read),
            cleanup,
        };
        match watch.replay(peer).await {
            Ok(()) => {
                watch.state.live();
                Ok(watch)
            }
            Err(code) => {
                let warnings = watch.close().await;
                Err(WatchFailure { code, warnings })
            }
        }
    }

    /// The opening replay: every REQ taken and every relay at EOSE.
    async fn replay(&mut self, peer: Option<&PublicKey>) -> Result<(), ErrorCode> {
        if let Some(peer) = peer {
            self.chat_with(peer)?;
        }
        for feed in self.feeds() {
            for relay in 0..self.relays.len() {
                if !self.send_req(relay, feed).await {
                    return Err(ErrorCode::RelayReadFailed);
                }
            }
        }
        self.settle().await
    }

    /// Follow the chat with `peer` as well, once it is learned while
    /// watching: its REQ goes to every relay that is connected, and returns
    /// when their replays end; the replayed records come back from the next
    /// [`Watch::next`]. A relay that is away, or does not take the REQ, gets
    /// it when it reconnects. Nothing changes when the chat is followed
    /// already. Only more records than the run allows fail it.
    pub async fn follow_chat(&mut self, peer: &PublicKey) -> Result<(), ErrorCode> {
        if self.chat.is_some() {
            return Ok(());
        }
        self.chat_with(peer)?;
        for relay in 0..self.relays.len() {
            if !self.state.is_down(relay) {
                self.send_req(relay, Feed::Chat).await;
            }
        }
        self.settle().await
    }

    /// The next new records, oldest first: at once when some are released,
    /// otherwise as soon as some arrive while no relay replays. With nothing
    /// new by `deadline` (`None`: no deadline), an empty batch. Only more
    /// records than the run allows fail it (`read_limit_exceeded`).
    pub async fn next(&mut self, deadline: Option<Instant>) -> Result<Batch, ErrorCode> {
        loop {
            self.state.expire(Instant::now())?;
            if let Some(batch) = self.state.release() {
                return Ok(batch);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(self.state.at_deadline());
            }
            let until = earliest(deadline, self.state.next_expiry());
            match self.armed.recv(until).await {
                Received::Notification(relay, notification) => {
                    self.handle(relay, notification).await?;
                }
                Received::Timeout => {}
                Received::Ended => {
                    self.state.lost()?;
                    if let Some(batch) = self.state.release() {
                        return Ok(batch);
                    }
                    match deadline {
                        Some(deadline) => sleep_until(deadline).await,
                        None => pending().await,
                    }
                    return Ok(self.state.at_deadline());
                }
            }
        }
    }

    /// Every record released so far, returned or skipped because the start
    /// cursor lists it: what [`Cursor::advance`] takes.
    pub fn observed(&self) -> Vec<Observed> {
        self.state.observed()
    }

    /// Unsubscribe and disconnect within the cleanup budget; an overrun is
    /// `cleanup_incomplete`.
    pub async fn close(self) -> Warnings {
        shut_down(&self.session, self.cleanup).await
    }

    /// Keep the conversation with `peer` and its filter.
    fn chat_with(&mut self, peer: &PublicKey) -> Result<(), ErrorCode> {
        let trade = &self.trade[0].1;
        let (conv, sign) = derive_chat_keys(trade, peer).map_err(|_| ErrorCode::InternalError)?;
        let sign = sign.public_key();
        let filter = chat_filter(sign).since(self.since).limit(PER_RELAY_LIMIT);
        let writers = [trade.public_key(), *peer];
        self.chat = Some(ChatKeys {
            conv,
            sign,
            writers,
            filter,
        });
        Ok(())
    }

    /// The feeds followed: the mail, and the chat once its peer is known.
    fn feeds(&self) -> Vec<Feed> {
        match self.chat {
            Some(_) => vec![Feed::Mail, Feed::Chat],
            None => vec![Feed::Mail],
        }
    }

    fn filter(&self, feed: Feed) -> Option<&Filter> {
        match feed {
            Feed::Mail => Some(&self.mail),
            Feed::Chat => self.chat.as_ref().map(|chat| &chat.filter),
        }
    }

    /// Send the filter of `feed` to `relay` alone, under a new id; its replay
    /// starts. `false` when the relay did not take the REQ.
    async fn send_req(&mut self, relay: usize, feed: Feed) -> bool {
        let Some(filter) = self.filter(feed) else {
            return false;
        };
        let id = SubscriptionId::generate();
        let target = ReqTarget::single(&self.relays[relay], [filter.clone()]);
        let sent = self
            .session
            .client
            .subscribe(target)
            .with_id(id.clone())
            .await;
        let taken = sent.is_ok_and(|output| output.failed.is_empty());
        if taken {
            self.state.subscribed(relay, feed, id, Instant::now());
        }
        taken
    }

    /// `relay` is back: everything it still holds replays, and each feed it
    /// no longer holds is sent again under a new id.
    async fn reconnected(&mut self, relay: usize) {
        let handle = self.session.client.relay(&self.relays[relay]).await;
        let held: HashSet<SubscriptionId> = match handle {
            Ok(Some(handle)) => handle.subscriptions().await.into_keys().collect(),
            _ => HashSet::new(),
        };
        let dropped = self.state.dropped(relay, &self.feeds(), &held);
        self.state.reconnected(relay, Instant::now());
        for feed in dropped {
            self.send_req(relay, feed).await;
        }
    }

    /// Drop `id` from what the SDK holds for `relay`: after a CLOSED it keeps
    /// some (`rate-limited:`) and would send them again on reconnect, next to
    /// the watch's own new id.
    async fn forget(&self, relay: usize, id: &SubscriptionId) {
        if let Ok(Some(handle)) = self.session.client.relay(&self.relays[relay]).await {
            let _ = handle.unsubscribe(id).await;
        }
    }

    /// Process notifications until no relay replays, for one replay budget
    /// at most: a replay a reconnect starts meanwhile is not waited for.
    async fn settle(&mut self) -> Result<(), ErrorCode> {
        let limit = self.state.replay_end(Instant::now());
        loop {
            let now = Instant::now();
            self.state.expire(now)?;
            if !self.state.replaying() || now >= limit {
                return Ok(());
            }
            match self
                .armed
                .recv(earliest(Some(limit), self.state.next_expiry()))
                .await
            {
                Received::Notification(relay, notification) => {
                    self.handle(relay, notification).await?;
                }
                Received::Timeout => {}
                Received::Ended => self.state.lost()?,
            }
        }
    }

    async fn handle(
        &mut self,
        relay: usize,
        notification: RelayNotification,
    ) -> Result<(), ErrorCode> {
        self.state.notified(relay)?;
        match notification {
            RelayNotification::Message { message } => match *message {
                RelayMessage::Event {
                    subscription_id,
                    event,
                } if self.state.is_ours(relay, &subscription_id) => {
                    let stored = (event.created_at.as_secs(), event.id);
                    if self.state.stored(relay, &subscription_id, stored) {
                        return Ok(());
                    }
                    if let Some(seen) = self.accept(&event).await {
                        self.state.event(relay, &subscription_id, seen)?;
                    }
                }
                RelayMessage::EndOfStoredEvents(id) => self.state.eose(relay, &id),
                RelayMessage::Closed {
                    subscription_id, ..
                } if self.state.is_ours(relay, &subscription_id) => {
                    self.state.closed(relay, &subscription_id)?;
                    self.forget(relay, &subscription_id).await;
                }
                _ => {}
            },
            RelayNotification::RelayStatus {
                status: RelayStatus::Disconnected | RelayStatus::Terminated | RelayStatus::Shutdown,
            } => self.state.disconnected(relay)?,
            RelayNotification::RelayStatus {
                status: RelayStatus::Connected,
            } => self.reconnected(relay).await,
            _ => {}
        }
        Ok(())
    }

    /// The record in `event`, routed by its author: the node's mail to the
    /// trade key, or a message of the followed chat, in an event dated since
    /// the window. A chat message may be written up to 60 s before its
    /// envelope, so only the envelope's time is held to the window.
    async fn accept(&self, event: &Event) -> Option<Seen> {
        if event.created_at < self.since {
            return None;
        }
        if event.pubkey == self.node {
            let one = BTreeSet::from([event.clone()]);
            let mut mail = open_events(&one, &self.trade, self.node, self.since).await;
            return mail.pop().map(|mail| Seen::Mail(Box::new(mail)));
        }
        let chat = self
            .chat
            .as_ref()
            .filter(|chat| event.pubkey == chat.sign)?;
        let keys = (&chat.conv, &chat.sign);
        let written = self
            .since
            .as_secs()
            .saturating_sub(CHAT_MAX_CLOCK_SKEW_SECS);
        let now = Timestamp::now();
        let (mut lines, _) = open_lines([event], keys, &chat.writers, written.into(), now);
        Some(Seen::Chat {
            outer_id: event.id,
            created_at: event.created_at.as_secs(),
            line: lines.pop()?,
        })
    }
}

/// The earlier of two optional instants.
fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Unsubscribe and disconnect within `cleanup`. A relay that is away cannot
/// be told to close; disconnecting it is enough, so only the overrun counts.
async fn shut_down(session: &Session, cleanup: Duration) -> Warnings {
    let mut warnings = Warnings::new();
    let closing = async {
        let _ = session.client.unsubscribe_all().await;
        session.client.shutdown().await;
    };
    if timeout(cleanup, closing).await.is_err() {
        warnings.add(WarningCode::CleanupIncomplete);
    }
    warnings
}
