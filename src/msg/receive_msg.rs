//! Bounded collection of public events from every configured relay.

use std::collections::BTreeSet;

use futures::stream::{select_all, StreamExt};
use nostr_sdk::prelude::{
    Event, Filter, MatchEventOptions, RelayMessage, RelayNotification, RelayStatus, SubscriptionId,
    Timestamp,
};
use tokio::time::{timeout, timeout_at, Instant};

use super::{PublicReadConfig, ReadLimits, Session};
use crate::outcome::order::Snapshot;
use crate::outcome::{ErrorCode, WarningCode, Warnings};

/// A read in which every relay reached EOSE for `filter`.
#[derive(Clone, Debug)]
pub struct Fetched {
    events: BTreeSet<Event>,
    snapshot: Snapshot,
    warnings: Warnings,
}

impl Fetched {
    /// Unique verified events matching the filter, newest first.
    pub fn events(&self) -> &BTreeSet<Event> {
        &self.events
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// A failed read. No events are returned, even when some were collected.
#[derive(Clone, Debug)]
pub struct FetchFailure {
    code: ErrorCode,
    snapshot: Option<Snapshot>,
    warnings: Warnings,
}

impl FetchFailure {
    fn before_query(code: ErrorCode) -> Self {
        Self {
            code,
            snapshot: None,
            warnings: Warnings::new(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// `None` when the read failed before the query started.
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Subscribe to `filter` on every configured relay and collect until each
/// relay's EOSE. Any relay failing, a deadline or the event cap fails the
/// whole read; nothing is retried.
pub async fn fetch_msg(
    config: &PublicReadConfig,
    filter: Filter,
    limits: &ReadLimits,
) -> Result<Fetched, FetchFailure> {
    let (Some(since), Some(per_relay_limit)) = (filter.since, filter.limit) else {
        return Err(FetchFailure::before_query(ErrorCode::InternalError));
    };
    let session = Session::connect(config, limits)
        .await
        .map_err(FetchFailure::before_query)?;
    let id = SubscriptionId::generate();
    let mut collection = Collection::new(&filter, &id, limits, config.relays().len());
    let result = collect(&session, &mut collection, config).await;
    let observed_at = Timestamp::now();

    let mut warnings = Warnings::new();
    let cleanup = async {
        let unsubscribed = session.client.unsubscribe(&id).await.is_ok();
        session.client.shutdown().await;
        unsubscribed
    };
    if !matches!(timeout(limits.cleanup, cleanup).await, Ok(true)) {
        warnings.add(WarningCode::CleanupIncomplete);
    }

    let snapshot = Snapshot::new(
        since,
        observed_at,
        per_relay_limit,
        limits.max_unique_events,
        config.relays().len(),
        collection.completed,
        collection.events.len(),
    );
    match result {
        Ok(()) => Ok(Fetched {
            events: collection.events,
            snapshot,
            warnings,
        }),
        Err(code) => Err(FetchFailure {
            code,
            snapshot: Some(snapshot),
            warnings,
        }),
    }
}

/// Arm every relay's notifications, send the REQ, then observe until each
/// relay's EOSE, a failure or the read deadline.
async fn collect(
    session: &Session,
    collection: &mut Collection<'_>,
    config: &PublicReadConfig,
) -> Result<(), ErrorCode> {
    let mut streams = Vec::with_capacity(config.relays().len());
    for (index, url) in config.relays().iter().enumerate() {
        let relay = session
            .client
            .relay(url)
            .await
            .ok()
            .flatten()
            .ok_or(ErrorCode::InternalError)?;
        streams.push(relay.notifications().map(move |n| (index, n)));
    }
    let mut notifications = select_all(streams);

    let deadline = Instant::now() + collection.limits.read;
    let sent = session
        .client
        .subscribe(collection.filter.clone())
        .with_id(collection.id.clone())
        .await;
    if sent.map_or(true, |output| !output.failed.is_empty()) {
        return Err(ErrorCode::RelayReadFailed);
    }
    loop {
        let next = timeout_at(deadline, notifications.next())
            .await
            .map_err(|_| ErrorCode::ReadTimeout)?;
        let (relay, notification) = next.ok_or(ErrorCode::RelayReadFailed)?;
        if collection.observe(relay, notification)? {
            return Ok(());
        }
    }
}

/// Collection state of one read; each relay is frozen at its first EOSE.
struct Collection<'a> {
    filter: &'a Filter,
    id: &'a SubscriptionId,
    limits: &'a ReadLimits,
    events: BTreeSet<Event>,
    completed: usize,
    relays: Vec<RelayProgress>,
}

#[derive(Clone, Copy, Default)]
struct RelayProgress {
    notifications: usize,
    done: bool,
}

impl<'a> Collection<'a> {
    fn new(
        filter: &'a Filter,
        id: &'a SubscriptionId,
        limits: &'a ReadLimits,
        relays: usize,
    ) -> Self {
        Self {
            filter,
            id,
            limits,
            events: BTreeSet::new(),
            completed: 0,
            relays: vec![RelayProgress::default(); relays],
        }
    }

    /// Apply one notification from `relay`; `Ok(true)` once every relay has
    /// reached EOSE.
    fn observe(
        &mut self,
        relay: usize,
        notification: RelayNotification,
    ) -> Result<bool, ErrorCode> {
        let progress = &mut self.relays[relay];
        if progress.done {
            return Ok(false);
        }
        // Lagged notifications are dropped silently, so a full buffer is a failure.
        progress.notifications += 1;
        if progress.notifications >= self.limits.notification_capacity {
            return Err(ErrorCode::RelayReadFailed);
        }
        match notification {
            RelayNotification::Message { message } => match *message {
                RelayMessage::Event {
                    subscription_id,
                    event,
                } if *subscription_id == *self.id => self.keep(event.into_owned()),
                RelayMessage::EndOfStoredEvents(subscription_id)
                    if *subscription_id == *self.id =>
                {
                    progress.done = true;
                    self.completed += 1;
                    Ok(self.completed == self.relays.len())
                }
                RelayMessage::Closed {
                    subscription_id, ..
                } if *subscription_id == *self.id => Err(ErrorCode::RelayReadFailed),
                _ => Ok(false),
            },
            RelayNotification::RelayStatus {
                status: RelayStatus::Disconnected | RelayStatus::Terminated | RelayStatus::Shutdown,
            } => Err(ErrorCode::RelayReadFailed),
            _ => Ok(false),
        }
    }

    /// Retain `event` if it matches the filter; the SDK has already verified it.
    fn keep(&mut self, event: Event) -> Result<bool, ErrorCode> {
        if !self.filter.match_event(&event, MatchEventOptions::new())
            || self.events.contains(&event)
        {
            return Ok(false);
        }
        if self.events.len() >= self.limits.max_unique_events {
            return Err(ErrorCode::ReadLimitExceeded);
        }
        self.events.insert(event);
        Ok(false)
    }
}
