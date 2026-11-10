//! Replays what the SDK notification stream yields after a lag. A `broadcast`
//! receiver that fell behind keeps only the newest notifications, and the SDK
//! skips the `Lagged` error without telling the collector.

use std::slice::from_ref;

use nostr_sdk::prelude::{
    Event, EventBuilder, Filter, FinalizeEvent, Keys, Kind, RelayMessage, RelayNotification,
    SubscriptionId,
};
use tokio::sync::broadcast::{self, error::TryRecvError};

use super::Collection;
use crate::msg::{channel_size, ReadLimits};
use crate::outcome::ErrorCode;

const CAPACITY: usize = 8;

struct Read {
    id: SubscriptionId,
    filter: Filter,
    limits: ReadLimits,
}

impl Read {
    fn new() -> Self {
        Self {
            id: SubscriptionId::generate(),
            filter: Filter::new().kind(Kind::TextNote),
            limits: ReadLimits {
                notification_capacity: CAPACITY,
                ..ReadLimits::default()
            },
        }
    }

    /// One relay's traffic: `stored` matching events, the EOSE, then `after`
    /// more events, all sent before the collector reads anything.
    fn traffic(&self, stored: usize, after: usize) -> Vec<RelayNotification> {
        let keys = Keys::generate();
        let event = |n: usize| {
            let event = EventBuilder::new(Kind::TextNote, n.to_string())
                .finalize(&keys)
                .unwrap();
            message(RelayMessage::event(self.id.clone(), event))
        };
        let mut traffic: Vec<_> = (0..stored).map(event).collect();
        traffic.push(message(RelayMessage::eose(self.id.clone())));
        traffic.extend((stored..stored + after).map(event));
        traffic
    }

    /// The collector's verdict when the relay's channel holds `channel`
    /// notifications: `Ok(true)` completes the read, `Ok(false)` never sees
    /// the EOSE (the read then times out).
    fn replay(&self, channel: usize, traffic: Vec<RelayNotification>) -> Result<bool, ErrorCode> {
        let (sender, mut receiver) = broadcast::channel(channel);
        for notification in traffic {
            sender.send(notification).unwrap();
        }
        let mut collection = Collection::new(&self.filter, &self.id, &self.limits, 1);
        loop {
            match receiver.try_recv() {
                Ok(notification) => {
                    if collection.observe(0, notification)? {
                        return Ok(true);
                    }
                }
                // What the SDK stream wrapper does with a lag.
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => return Ok(false),
            }
        }
    }
}

fn message(message: RelayMessage<'static>) -> RelayNotification {
    RelayNotification::Message {
        message: Box::new(message),
    }
}

#[test]
fn the_channel_has_headroom_beyond_the_capacity() {
    let read = Read::new();
    assert_eq!(channel_size(&read.limits), 2 * CAPACITY);
    assert_eq!(
        channel_size(&ReadLimits::default()),
        2 * ReadLimits::default().notification_capacity
    );
}

#[test]
fn a_relay_within_the_capacity_completes() {
    let read = Read::new();
    let traffic = read.traffic(CAPACITY - 2, 3);
    assert_eq!(read.replay(channel_size(&read.limits), traffic), Ok(true));
}

#[test]
fn a_loss_hidden_by_traffic_after_eose_fails_the_relay() {
    // 40 stored events overflow the channel, and 4 events after the EOSE
    // follow before the collector reads.
    let read = Read::new();
    // Without headroom the EOSE lands 4 notifications into the kept window,
    // below the capacity, and 36 events are lost silently.
    assert_eq!(read.replay(CAPACITY, read.traffic(40, 4)), Ok(true));
    // With headroom the collector reaches the capacity before the EOSE.
    assert_eq!(
        read.replay(channel_size(&read.limits), read.traffic(40, 4)),
        Err(ErrorCode::RelayReadFailed)
    );
}

#[test]
fn traffic_after_eose_up_to_the_capacity_cannot_hide_a_loss() {
    let read = Read::new();
    for after in 0..=CAPACITY {
        assert_eq!(
            read.replay(channel_size(&read.limits), read.traffic(40, after)),
            Err(ErrorCode::RelayReadFailed),
            "{after} notifications after EOSE"
        );
    }
}

#[test]
fn more_traffic_after_eose_than_the_capacity_still_hides_a_loss() {
    // The documented limit of the mitigation: no SDK API reports a lag, so a
    // relay that floods before and after its EOSE on purpose can still hide
    // the events it pushed out of the channel.
    let read = Read::new();
    let traffic = read.traffic(40, CAPACITY + 1);
    assert_eq!(read.replay(channel_size(&read.limits), traffic), Ok(true));
}

// Spec 022: whether a relay delivered the filter's limit of matching events.

const LIMIT: usize = 3;

fn notes(keys: &Keys, from: usize, to: usize) -> Vec<Event> {
    (from..to)
        .map(|n| {
            EventBuilder::new(Kind::TextNote, n.to_string())
                .finalize(keys)
                .unwrap()
        })
        .collect()
}

/// Whether the read reached the limit once each relay delivered its events
/// and then its EOSE; `late` is delivered by relay 0 after its EOSE.
fn reached(relays: &[Vec<Event>], late: &[Event]) -> bool {
    let id = SubscriptionId::generate();
    let filter = Filter::new().kind(Kind::TextNote).limit(LIMIT);
    let limits = ReadLimits::default();
    let mut collection = Collection::new(&filter, &id, &limits, relays.len());
    let event = |event: &Event| message(RelayMessage::event(id.clone(), event.clone()));
    for (relay, events) in relays.iter().enumerate() {
        for e in events {
            collection.observe(relay, event(e)).unwrap();
        }
        collection
            .observe(relay, message(RelayMessage::eose(id.clone())))
            .unwrap();
    }
    for e in late {
        collection.observe(0, event(e)).unwrap();
    }
    collection.reached_limit()
}

#[test]
fn a_relay_delivering_the_limit_reaches_it() {
    let keys = Keys::generate();
    assert!(reached(&[notes(&keys, 0, LIMIT)], &[]));
    assert!(!reached(&[notes(&keys, 0, LIMIT - 1)], &[]));
}

#[test]
fn relays_below_the_limit_do_not_reach_it_together() {
    let keys = Keys::generate();
    let relays = [notes(&keys, 0, LIMIT - 1), notes(&keys, 10, 10 + LIMIT - 1)];
    assert!(!reached(&relays, &[]));
}

#[test]
fn an_event_another_relay_delivered_still_counts_for_this_relay() {
    let keys = Keys::generate();
    let shared = notes(&keys, 0, LIMIT);
    let relays = [shared[..LIMIT - 1].to_vec(), shared.clone()];
    assert!(reached(&relays, &[]));
}

#[test]
fn events_after_the_eose_or_outside_the_filter_do_not_count() {
    let keys = Keys::generate();
    let stored = notes(&keys, 0, LIMIT - 1);
    assert!(!reached(from_ref(&stored), &notes(&keys, 10, 11)));
    let other = EventBuilder::new(Kind::Metadata, "{}")
        .finalize(&keys)
        .unwrap();
    let mut relay = stored;
    relay.push(other);
    assert!(!reached(&[relay], &[]));
}
