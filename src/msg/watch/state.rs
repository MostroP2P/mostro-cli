//! The bookkeeping of a watch, without I/O (spec 020 D4, D7, D8): the
//! subscriptions each relay holds and which of them are replaying, the
//! records held back while any relay replays, the records already seen, the
//! ids the start cursor lists, what was observed, and whether a replay may
//! have lost records.
//!
//! The opening replay is strict, as for the reads: a CLOSED or a disconnect
//! before EOSE, the notification capacity or the read budget fails the watch.
//! Once live, none of them does; the replay just ends. A reconnect starts a
//! replay of every subscription the relay still holds (D5).

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::time::Duration;

use nostr_sdk::prelude::{EventId, SubscriptionId};
use tokio::time::Instant;

use crate::msg::orders::PER_RELAY_LIMIT;
use crate::msg::ReadLimits;
use crate::outcome::watch::Observed;
use crate::outcome::ErrorCode;

use super::records::{Batch, Seen};

/// What a subscription follows: the node's mail or the chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Feed {
    Mail,
    Chat,
}

/// One subscription of the watch on one relay.
struct Sub {
    id: SubscriptionId,
    feed: Feed,
    replay: Option<Replay>,
}

/// A replay in progress.
#[derive(Clone, Copy)]
struct Replay {
    until: Instant,
    /// Events the relay sent under the subscription, opened or not.
    stored: usize,
    /// The oldest of them, by outer time and id, and whether it opened as a
    /// record known before.
    oldest: Option<(u64, EventId, bool)>,
    /// Notifications from the relay since the replay started.
    notifications: usize,
}

impl Replay {
    /// The relay sent its limit and the oldest of them was new: older records
    /// may be missing. A live replay that ends before EOSE (a disconnect, its
    /// budget or the notifications ending) is judged the same way.
    fn cut_short(&self) -> bool {
        let oldest_new = self.oldest.is_some_and(|(_, _, known)| !known);
        self.stored >= PER_RELAY_LIMIT && oldest_new
    }
}

/// A record not yet released: its first arrival, whether that was new, and
/// the newest outer time it arrived at with what that observes.
struct Held {
    seen: Seen,
    new: bool,
    newest: (u64, Observed),
}

pub(super) struct State {
    relays: Vec<Vec<Sub>>,
    /// The relays that went away and have not reconnected.
    down: Vec<bool>,
    /// Records not yet released, one entry each however often they arrive.
    held: HashMap<EventId, Held>,
    /// Records seen in this run: node messages by event id, chat by inner id.
    mail: HashSet<EventId>,
    chat: HashSet<EventId>,
    /// The ids the start cursor lists: already returned by an earlier call.
    skip: HashSet<EventId>,
    /// Released records by id, each at the newest outer time seen.
    observed: HashMap<EventId, (u64, Observed)>,
    truncated: bool,
    strict: bool,
    replay: Duration,
    capacity: usize,
    max_records: usize,
}

impl State {
    pub(super) fn new(relays: usize, skip: &[EventId], limits: &ReadLimits) -> Self {
        Self {
            relays: (0..relays).map(|_| Vec::new()).collect(),
            down: vec![false; relays],
            held: HashMap::new(),
            mail: HashSet::new(),
            chat: HashSet::new(),
            skip: skip.iter().copied().collect(),
            observed: HashMap::new(),
            truncated: false,
            strict: true,
            replay: limits.read,
            capacity: limits.notification_capacity,
            max_records: limits.max_unique_events,
        }
    }

    /// The opening replay is over: from now on no relay event fails the watch.
    pub(super) fn live(&mut self) {
        self.strict = false;
    }

    /// `relay` took the REQ `id` for `feed`, sent at `now`: its replay starts.
    /// It replaces the feed's earlier subscription there.
    pub(super) fn subscribed(
        &mut self,
        relay: usize,
        feed: Feed,
        id: SubscriptionId,
        now: Instant,
    ) {
        let replay = Some(self.replay_from(now));
        self.relays[relay].retain(|sub| sub.feed != feed);
        self.relays[relay].push(Sub { id, feed, replay });
    }

    /// `relay` is back at `now`: the SDK sends again every subscription it
    /// still holds, so each of them replays.
    pub(super) fn reconnected(&mut self, relay: usize, now: Instant) {
        self.down[relay] = false;
        let replay = self.replay_from(now);
        for sub in &mut self.relays[relay] {
            sub.replay = Some(replay);
        }
    }

    /// Whether `relay` went away and has not reconnected.
    pub(super) fn is_down(&self, relay: usize) -> bool {
        self.down[relay]
    }

    /// The feeds of `feeds` whose subscription `relay` no longer holds, by
    /// the ids the SDK keeps there (`held`): after a CLOSED, or never sent
    /// while it was away. Their stale entries are forgotten.
    pub(super) fn dropped(
        &mut self,
        relay: usize,
        feeds: &[Feed],
        held: &HashSet<SubscriptionId>,
    ) -> Vec<Feed> {
        self.relays[relay].retain(|sub| held.contains(&sub.id));
        let kept = &self.relays[relay];
        let missing = |feed: &&Feed| !kept.iter().any(|sub| sub.feed == **feed);
        feeds.iter().filter(missing).copied().collect()
    }

    fn replay_from(&self, now: Instant) -> Replay {
        Replay {
            until: now + self.replay,
            stored: 0,
            oldest: None,
            notifications: 0,
        }
    }

    pub(super) fn is_ours(&self, relay: usize, id: &SubscriptionId) -> bool {
        self.relays[relay].iter().any(|sub| sub.id == *id)
    }

    pub(super) fn replaying(&self) -> bool {
        self.relays.iter().flatten().any(|sub| sub.replay.is_some())
    }

    /// When the first replay still running runs out of time.
    pub(super) fn next_expiry(&self) -> Option<Instant> {
        let replays = self.relays.iter().flatten();
        replays
            .filter_map(|sub| Some(sub.replay.as_ref()?.until))
            .min()
    }

    /// One notification from `relay`. The SDK drops what overflows its channel
    /// silently, so a replay that reaches the capacity may have lost events.
    pub(super) fn notified(&mut self, relay: usize) -> Result<(), ErrorCode> {
        let mut full = false;
        for sub in &mut self.relays[relay] {
            if let Some(replay) = &mut sub.replay {
                replay.notifications += 1;
                if replay.notifications >= self.capacity {
                    sub.replay = None;
                    full = true;
                }
            }
        }
        if full {
            self.strictly(ErrorCode::RelayReadFailed)?;
            self.truncated = true;
        }
        Ok(())
    }

    /// The event `event`, dated `time`, that `relay` sent under `id`, before
    /// it is opened: it counts toward the replay's stored events, as for the
    /// reads, whether it opens or not. `true` for a node message already seen
    /// in this run, which needs no opening again.
    pub(super) fn stored(
        &mut self,
        relay: usize,
        id: &SubscriptionId,
        (time, event): (u64, EventId),
    ) -> bool {
        let known = self.mail.contains(&event);
        let sub = self.relays[relay].iter_mut().find(|sub| sub.id == *id);
        if let Some(replay) = sub.and_then(|sub| sub.replay.as_mut()) {
            replay.stored += 1;
            if replay
                .oldest
                .is_none_or(|(at, id, _)| (time, event) < (at, id))
            {
                replay.oldest = Some((time, event, known));
            }
        }
        known
    }

    /// A record opened from an event `relay` sent under `id`; one under an id
    /// the relay does not hold for the watch is ignored. Past
    /// `max_unique_events` records in the run, `read_limit_exceeded`.
    pub(super) fn event(
        &mut self,
        relay: usize,
        id: &SubscriptionId,
        seen: Seen,
    ) -> Result<(), ErrorCode> {
        let Some(sub) = self.relays[relay].iter_mut().find(|sub| sub.id == *id) else {
            return Ok(());
        };
        let record = seen.id();
        let seen_set = match seen {
            Seen::Mail(_) => &mut self.mail,
            Seen::Chat { .. } => &mut self.chat,
        };
        let known = self.skip.contains(&record) || !seen_set.insert(record);
        let oldest = sub
            .replay
            .as_mut()
            .and_then(|replay| replay.oldest.as_mut());
        if let Some(oldest) = oldest.filter(|oldest| oldest.1 == seen.outer_id()) {
            oldest.2 = known;
        }
        if self.mail.len() + self.chat.len() > self.max_records {
            return Err(ErrorCode::ReadLimitExceeded);
        }
        let newest = (seen.created_at(), seen.observed());
        match self.held.entry(record) {
            Entry::Occupied(mut held) if newest.0 > held.get().newest.0 => {
                held.get_mut().newest = newest;
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(slot) => {
                let new = !known;
                slot.insert(Held { seen, new, newest });
            }
        }
        Ok(())
    }

    /// `relay` sent every stored event of `id`: older records may be missing
    /// when the replay was cut short.
    pub(super) fn eose(&mut self, relay: usize, id: &SubscriptionId) {
        let sub = self.relays[relay].iter_mut().find(|sub| sub.id == *id);
        if let Some(replay) = sub.and_then(|sub| sub.replay.take()) {
            self.truncated |= replay.cut_short();
        }
    }

    /// `relay` closed `id`: the watch no longer has it there.
    pub(super) fn closed(&mut self, relay: usize, id: &SubscriptionId) -> Result<(), ErrorCode> {
        let Some(at) = self.relays[relay].iter().position(|sub| sub.id == *id) else {
            return Ok(());
        };
        if self.relays[relay][at].replay.is_some() {
            self.strictly(ErrorCode::RelayReadFailed)?;
        }
        self.relays[relay].remove(at);
        Ok(())
    }

    /// `relay` went away: its replays end until it reconnects.
    pub(super) fn disconnected(&mut self, relay: usize) -> Result<(), ErrorCode> {
        if self.relays[relay].iter().any(|sub| sub.replay.is_some()) {
            self.strictly(ErrorCode::RelayReadFailed)?;
        }
        self.down[relay] = true;
        for sub in &mut self.relays[relay] {
            self.truncated |= sub.replay.take().is_some_and(|r| r.cut_short());
        }
        Ok(())
    }

    /// Every relay's notifications ended: every replay ends.
    pub(super) fn lost(&mut self) -> Result<(), ErrorCode> {
        self.strictly(ErrorCode::RelayReadFailed)?;
        for sub in self.relays.iter_mut().flatten() {
            self.truncated |= sub.replay.take().is_some_and(|r| r.cut_short());
        }
        Ok(())
    }

    /// The replays whose budget ran out by `now` end; in the opening replay
    /// that is `read_timeout`.
    pub(super) fn expire(&mut self, now: Instant) -> Result<(), ErrorCode> {
        if self.next_expiry().is_some_and(|until| until <= now) {
            self.strictly(ErrorCode::ReadTimeout)?;
        }
        for sub in self.relays.iter_mut().flatten() {
            if sub
                .replay
                .as_ref()
                .is_some_and(|replay| replay.until <= now)
            {
                self.truncated |= sub.replay.take().is_some_and(|r| r.cut_short());
            }
        }
        Ok(())
    }

    /// The held records once no relay replays: the new ones, oldest first, or
    /// `None` when none is new. Every record released counts as observed.
    pub(super) fn release(&mut self) -> Option<Batch> {
        if self.replaying() {
            return None;
        }
        let mut items = Vec::new();
        for (id, held) in self.held.drain() {
            let entry = self.observed.entry(id).or_insert(held.newest);
            if held.newest.0 > entry.0 {
                *entry = held.newest;
            }
            if held.new {
                items.push(held.seen);
            }
        }
        if items.is_empty() {
            return None;
        }
        items.sort_by_key(|seen| (seen.created_at(), seen.id()));
        Some(Batch {
            items,
            truncated: take(&mut self.truncated),
        })
    }

    /// What a call that reaches its deadline gets: no records, and whether a
    /// replay may have lost some. Held records stay held and unobserved.
    pub(super) fn at_deadline(&mut self) -> Batch {
        Batch {
            items: Vec::new(),
            truncated: take(&mut self.truncated),
        }
    }

    /// Every record released so far, returned or skipped.
    pub(super) fn observed(&self) -> Vec<Observed> {
        self.observed
            .values()
            .map(|(_, observed)| *observed)
            .collect()
    }

    fn strictly(&self, code: ErrorCode) -> Result<(), ErrorCode> {
        if self.strict {
            Err(code)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use mostro_core::prelude::{Action, Message};
    use nostr_sdk::prelude::Keys;
    use uuid::Uuid;

    use super::*;
    use crate::msg::chat::ChatLine;
    use crate::msg::inbox::Mail;
    use crate::outcome::watch::Cursor;

    const NOW: u64 = 1_792_108_800;

    fn id(n: u16) -> EventId {
        let mut bytes = [0u8; 32];
        bytes[..2].copy_from_slice(&n.to_be_bytes());
        EventId::from_byte_array(bytes)
    }

    fn sub(name: &str) -> SubscriptionId {
        SubscriptionId::new(name)
    }

    fn mail(n: u16, at: u64) -> Seen {
        let message = Message::new_order(
            Some(Uuid::from_u128(1)),
            None,
            None,
            Action::WaitingSellerToPay,
            None,
        );
        Seen::Mail(Box::new(Mail {
            event: id(n),
            created_at: at,
            message,
            index: Some(1),
            key: Keys::generate().public_key(),
        }))
    }

    /// The chat message `n`, written at `inner`, in the envelope `outer_id`
    /// dated `at`.
    fn chat(n: u16, outer_id: u16, at: u64, inner: u64) -> Seen {
        Seen::Chat {
            outer_id: id(outer_id),
            created_at: at,
            line: ChatLine {
                message_id: id(n),
                sender: Keys::generate().public_key(),
                created_at: inner,
                text: n.to_string(),
            },
        }
    }

    /// What the watch does with an event from `relay` under `name` that
    /// opens as `seen`.
    fn deliver(state: &mut State, relay: usize, name: &str, seen: Seen) -> Result<(), ErrorCode> {
        state.stored(relay, &sub(name), (seen.created_at(), seen.outer_id()));
        state.event(relay, &sub(name), seen)
    }

    fn got(state: &mut State, relay: usize, name: &str, seen: Seen) {
        deliver(state, relay, name, seen).unwrap();
    }

    fn limits() -> ReadLimits {
        ReadLimits {
            read: Duration::from_secs(15),
            notification_capacity: 8,
            max_unique_events: 100,
            ..ReadLimits::default()
        }
    }

    /// A state past its opening replay, with `relays` relays.
    fn live(relays: usize, skip: &[EventId]) -> (State, Instant) {
        let mut state = State::new(relays, skip, &limits());
        state.live();
        (state, Instant::now())
    }

    fn ids(batch: &Batch) -> Vec<EventId> {
        batch.items.iter().map(Seen::id).collect()
    }

    /// The cursor a call that saw what `state` observed leaves at `now`.
    fn cursor(state: &State) -> Cursor {
        Cursor::start(0).advance(&state.observed(), NOW)
    }

    #[test]
    fn records_are_held_until_every_replaying_relay_ends_its_replay() {
        let mut state = State::new(2, &[], &limits());
        let start = Instant::now();
        state.subscribed(0, Feed::Mail, sub("a"), start);
        state.subscribed(1, Feed::Mail, sub("b"), start);

        got(&mut state, 0, "a", mail(1, 100));
        state.eose(0, &sub("a"));
        assert!(state.release().is_none(), "relay 1 still replays");
        got(&mut state, 1, "b", mail(2, 90));
        assert!(state.observed().is_empty(), "held, not observed");
        state.eose(1, &sub("b"));
        let opening = state.release().unwrap();
        assert_eq!(ids(&opening), [id(2), id(1)], "oldest first");
        assert!(!opening.truncated);
        assert!(state.release().is_none(), "nothing new");

        state.live();
        state.subscribed(1, Feed::Mail, sub("c"), start);
        got(&mut state, 0, "a", mail(3, 200));
        assert!(state.release().is_none(), "held while relay 1 replays");
        assert!(state.at_deadline().items.is_empty());
        assert_eq!(state.observed().len(), 2, "a held record is not observed");
        state.eose(1, &sub("c"));
        assert_eq!(ids(&state.release().unwrap()), [id(3)]);
        assert_eq!(state.observed().len(), 3);
        got(&mut state, 0, "a", mail(4, 300));
        assert_eq!(ids(&state.release().unwrap()), [id(4)], "live at once");
    }

    #[test]
    fn a_replay_ends_at_eose_disconnect_or_fifteen_seconds() {
        let (mut state, start) = live(3, &[]);
        for (relay, name) in ["a", "b", "c"].into_iter().enumerate() {
            state.subscribed(relay, Feed::Mail, sub(name), start);
        }
        assert_eq!(state.next_expiry(), Some(start + Duration::from_secs(15)));
        state.eose(0, &sub("a"));
        state.disconnected(1).unwrap();
        state.expire(start + Duration::from_secs(14)).unwrap();
        assert!(state.replaying(), "relay 2 has 1 s left");
        state.expire(start + Duration::from_secs(15)).unwrap();
        assert!(!state.replaying());
        assert_eq!(state.next_expiry(), None);

        state.subscribed(0, Feed::Chat, sub("d"), start);
        state.closed(0, &sub("d")).unwrap();
        assert!(!state.replaying(), "a CLOSED ends it and drops it");
        assert!(!state.is_ours(0, &sub("d")));
        assert!(state.is_ours(0, &sub("a")));
        got(&mut state, 0, "d", mail(1, 100));
        assert!(state.release().is_none(), "nothing under a dropped id");

        state.subscribed(2, Feed::Mail, sub("e"), start);
        for _ in 0..limits().notification_capacity {
            state.notified(2).unwrap();
        }
        assert!(!state.replaying(), "the capacity ends it");
        assert!(state.at_deadline().truncated, "and may have lost events");
    }

    #[test]
    fn the_opening_replay_is_strict() {
        let start = Instant::now();
        let opening = || {
            let mut state = State::new(2, &[], &limits());
            state.subscribed(0, Feed::Mail, sub("a"), start);
            state.subscribed(1, Feed::Mail, sub("b"), start);
            state
        };
        let mut state = opening();
        assert_eq!(state.disconnected(1), Err(ErrorCode::RelayReadFailed));
        let mut state = opening();
        assert_eq!(state.closed(0, &sub("a")), Err(ErrorCode::RelayReadFailed));
        assert_eq!(state.closed(0, &sub("x")), Ok(()), "not ours");
        let mut state = opening();
        assert_eq!(
            state.expire(start + Duration::from_secs(15)),
            Err(ErrorCode::ReadTimeout)
        );
        let mut state = opening();
        let filled = (0..limits().notification_capacity).map(|_| state.notified(0));
        assert_eq!(
            filled.last(),
            Some(Err(ErrorCode::RelayReadFailed)),
            "the SDK drops what overflows, silently"
        );
        let mut state = opening();
        state.eose(0, &sub("a"));
        assert_eq!(state.closed(0, &sub("a")), Ok(()), "after its EOSE");
        assert_eq!(state.lost(), Err(ErrorCode::RelayReadFailed));
    }

    #[test]
    fn mail_counts_once_by_outer_id_and_chat_once_by_message_id() {
        let (mut state, start) = live(2, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        state.subscribed(1, Feed::Mail, sub("b"), start);
        state.eose(0, &sub("a"));
        state.eose(1, &sub("b"));

        got(&mut state, 0, "a", mail(1, 100));
        got(&mut state, 1, "b", mail(1, 100));
        got(&mut state, 0, "a", chat(7, 70, 50, 20));
        let first = state.release().unwrap();
        assert_eq!(ids(&first), [id(7), id(1)], "by outer time");
        got(&mut state, 1, "b", chat(7, 70, 50, 20));
        got(&mut state, 1, "b", chat(7, 71, 200, 20));
        assert!(
            state.release().is_none(),
            "the same line in a second envelope"
        );

        let next = cursor(&state);
        assert_eq!(next.since(), 140, "the newest envelope of the line counts");
        assert_eq!(next.ids(), [id(7)]);
    }

    #[test]
    fn repeats_while_a_relay_replays_are_held_once() {
        let (mut state, start) = live(2, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        state.subscribed(1, Feed::Mail, sub("b"), start);
        state.eose(1, &sub("b"));
        for n in 0..1_000 {
            got(&mut state, 1, "b", mail(1, 100));
            got(&mut state, 1, "b", chat(7, n, 100 + u64::from(n), 100));
        }
        assert_eq!(state.held.len(), 2, "one entry per record");
        state.eose(0, &sub("a"));
        assert_eq!(ids(&state.release().unwrap()), [id(1), id(7)]);
        assert_eq!(state.observed[&id(7)].0, 1_099, "the newest envelope");
    }

    #[test]
    fn cursor_ids_are_skipped_but_observed() {
        let (mut state, start) = live(1, &[id(1), id(7)]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        got(&mut state, 0, "a", mail(1, NOW - 10));
        got(&mut state, 0, "a", chat(7, 70, NOW - 10, NOW - 10));
        state.eose(0, &sub("a"));
        assert!(state.release().is_none(), "both were returned before");
        assert_eq!(cursor(&state).ids(), [id(1), id(7)], "and stay listed");

        got(&mut state, 0, "a", mail(2, NOW - 5));
        assert_eq!(ids(&state.release().unwrap()), [id(2)]);
        assert_eq!(cursor(&state).ids(), [id(1), id(2), id(7)]);
    }

    #[test]
    fn the_seen_cap_is_read_limit_exceeded() {
        let small = ReadLimits {
            max_unique_events: 3,
            ..limits()
        };
        let mut state = State::new(1, &[id(9)], &small);
        state.live();
        state.subscribed(0, Feed::Mail, sub("a"), Instant::now());
        for n in 1..=3 {
            got(&mut state, 0, "a", mail(n, 100));
        }
        got(&mut state, 0, "a", mail(1, 100));
        got(&mut state, 0, "a", mail(9, 100));
        assert_eq!(
            deliver(&mut state, 0, "a", chat(4, 40, 100, 100)),
            Err(ErrorCode::ReadLimitExceeded),
            "a fourth record, after a repeat and a cursor id"
        );
    }

    #[test]
    fn truncated_only_when_the_oldest_of_fifty_is_new() {
        let replay = |known: &[EventId], stored: u16| {
            let (mut state, start) = live(1, known);
            state.subscribed(0, Feed::Mail, sub("a"), start);
            for n in 1..=stored {
                got(&mut state, 0, "a", mail(n, 1_000 + u64::from(n)));
            }
            state.eose(0, &sub("a"));
            state.at_deadline().truncated
        };
        assert!(replay(&[], 50), "the relay's limit, and the oldest is new");
        assert!(!replay(&[], 49));
        assert!(!replay(&[id(1)], 50), "the oldest was listed in the cursor");
        assert!(replay(&[id(2)], 50), "only the oldest tells");
        let (mut state, start) = live(1, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        for n in 1..=50 {
            state.stored(0, &sub("a"), (1_000 + u64::from(n), id(n)));
        }
        state.eose(0, &sub("a"));
        assert!(
            state.at_deadline().truncated,
            "events that do not open count"
        );

        let (mut state, start) = live(1, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        got(&mut state, 0, "a", mail(1, 1_001));
        state.eose(0, &sub("a"));
        assert!(state.release().is_some());
        state.subscribed(0, Feed::Mail, sub("b"), start);
        for n in 1..=50 {
            got(&mut state, 0, "b", mail(n, 1_000 + u64::from(n)));
        }
        state.eose(0, &sub("b"));
        let batch = state.release().unwrap();
        assert_eq!(batch.items.len(), 49);
        assert!(!batch.truncated, "the oldest was seen in this run");
    }

    #[test]
    fn a_reconnect_starts_a_replay_and_holds_records() {
        let (mut state, start) = live(2, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        state.subscribed(1, Feed::Mail, sub("b"), start);
        state.eose(0, &sub("a"));
        state.eose(1, &sub("b"));
        state.disconnected(0).unwrap();
        assert!(state.is_down(0));
        got(&mut state, 1, "b", mail(1, 100));
        assert_eq!(
            ids(&state.release().unwrap()),
            [id(1)],
            "away, it holds nothing"
        );

        let back = start + Duration::from_secs(5);
        state.reconnected(0, back);
        assert!(!state.is_down(0));
        assert_eq!(state.next_expiry(), Some(back + Duration::from_secs(15)));
        got(&mut state, 1, "b", mail(2, 200));
        got(&mut state, 0, "a", mail(1, 100));
        got(&mut state, 0, "a", mail(3, 150));
        assert!(
            state.release().is_none(),
            "held while relay 0 replays again"
        );
        state.eose(0, &sub("a"));
        let resent = state.release().unwrap();
        assert_eq!(ids(&resent), [id(3), id(2)], "what was missed, once");
        assert!(!resent.truncated);

        state.disconnected(0).unwrap();
        state.reconnected(0, back);
        for n in 10..60 {
            got(&mut state, 0, "a", mail(n, 1_000 + u64::from(n)));
        }
        state.eose(0, &sub("a"));
        let flooded = state.release().unwrap();
        assert_eq!(flooded.items.len(), 50);
        assert!(flooded.truncated, "fifty new events after a reconnect");
    }

    #[test]
    fn a_live_replay_of_fifty_cut_before_eose_sets_truncated() {
        let cut = |end: &dyn Fn(&mut State, Instant), stored: u16| {
            let (mut state, start) = live(1, &[]);
            state.subscribed(0, Feed::Mail, sub("a"), start);
            state.eose(0, &sub("a"));
            state.disconnected(0).unwrap();
            state.reconnected(0, start);
            for n in 1..=stored {
                got(&mut state, 0, "a", mail(n, 1_000 + u64::from(n)));
            }
            end(&mut state, start);
            state.release().unwrap().truncated
        };
        let away = |state: &mut State, _| state.disconnected(0).unwrap();
        let slow = |state: &mut State, start| {
            state.expire(start + Duration::from_secs(15)).unwrap();
        };
        let ended = |state: &mut State, _| state.lost().unwrap();
        let refused = |state: &mut State, _| state.closed(0, &sub("a")).unwrap();
        assert!(cut(&away, 50), "a second disconnect");
        assert!(cut(&refused, 50), "a CLOSED");
        assert!(cut(&slow, 50), "the fifteen seconds");
        assert!(cut(&ended, 50), "the notifications");
        assert!(!cut(&away, 49), "short of the relay's limit");
    }

    #[test]
    fn only_subscriptions_the_relay_dropped_are_sent_again() {
        let (mut state, start) = live(1, &[]);
        state.subscribed(0, Feed::Mail, sub("a"), start);
        state.subscribed(0, Feed::Chat, sub("c"), start);
        let feeds = [Feed::Mail, Feed::Chat];
        let held = |names: &[&str]| names.iter().map(|name| sub(name)).collect::<HashSet<_>>();

        let none: [Feed; 0] = [];
        assert_eq!(state.dropped(0, &feeds, &held(&["a", "c"])), none);
        assert_eq!(state.dropped(0, &feeds, &held(&["a"])), [Feed::Chat]);
        assert!(!state.is_ours(0, &sub("c")), "the stale entry is forgotten");
        assert!(state.is_ours(0, &sub("a")));
        assert_eq!(
            state.dropped(0, &feeds, &held(&["a"])),
            [Feed::Chat],
            "until it is sent again"
        );
        assert_eq!(state.dropped(0, &[Feed::Mail], &held(&["a"])), none);

        state.subscribed(0, Feed::Chat, sub("d"), start);
        assert_eq!(state.dropped(0, &feeds, &held(&["a", "d"])), none);
        state.subscribed(0, Feed::Mail, sub("e"), start);
        assert!(
            !state.is_ours(0, &sub("a")),
            "a new id replaces the old one"
        );
        assert_eq!(state.dropped(0, &feeds, &held(&["a", "d"])), [Feed::Mail]);
    }
}
