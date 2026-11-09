//! The cursor of `order.watch`: where the next JSON call starts, so that it
//! returns only records the earlier calls did not.

use std::collections::BTreeMap;
use std::fmt;

use mostro_core::prelude::CHAT_MAX_CLOCK_SKEW_SECS;
use nostr_sdk::prelude::EventId;

use super::InvalidWatch;

/// How far, at least, a cursor stays behind the newest record it covers: as
/// far as a chat message may be dated ahead of its envelope.
pub const CURSOR_MARGIN_SECS: u64 = CHAT_MAX_CLOCK_SKEW_SECS;
/// Record ids a cursor lists at most.
pub const MAX_CURSOR_IDS: usize = 200;
/// The longest cursor: a 20-digit time, a colon and 200 ids with their commas.
pub const MAX_CURSOR_LEN: usize = 20 + 1 + MAX_CURSOR_IDS * 65 - 1;
/// The wire form of a cursor. The 64-bit range of the time and the limit of
/// 200 ids are enforced by the producer and by the maximum length.
pub const CURSOR_PATTERN: &str = "^(0|[1-9][0-9]{0,19})(:[0-9a-f]{64}(,[0-9a-f]{64})*)?$";

/// A record a call accepted, as [`Cursor::advance`] counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observed {
    id: EventId,
    /// The outer `created_at`, which relays filter on.
    time: u64,
    /// The latest outer time another envelope of the record can carry.
    last: u64,
    /// A node message, listed ahead of chat messages when the cap binds.
    mail: bool,
}

impl Observed {
    /// A node message, by its event id: a re-published event keeps its time.
    pub fn mail(id: EventId, time: u64) -> Self {
        Self {
            id,
            time,
            last: time,
            mail: true,
        }
    }

    /// A chat message, by its inner id, in an envelope dated `outer` and
    /// written at `inner`: another envelope of it can be dated up to
    /// [`CHAT_MAX_CLOCK_SKEW_SECS`] after `inner`.
    pub fn chat(id: EventId, outer: u64, inner: u64) -> Self {
        Self {
            id,
            time: outer,
            last: outer.max(inner.saturating_add(CHAT_MAX_CLOCK_SKEW_SECS)),
            mail: false,
        }
    }
}

/// Where the next JSON call starts: a time in Unix seconds, and the ids of the
/// records at or after it that were already observed (node messages first,
/// then the newest, when more than 200 were). Callers treat it as opaque and pass it back verbatim with
/// `--after`.
///
/// The wire form is `<since>` or `<since>:<id>,<id>...`: decimal seconds with
/// no sign and no leading zeros, then 1 to 200 distinct lowercase 64-hex ids,
/// written sorted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    since: u64,
    /// Sorted, distinct, at most [`MAX_CURSOR_IDS`].
    ids: Vec<EventId>,
}

impl Cursor {
    /// A cursor at `since` with no ids, where a call without `--after` starts.
    pub fn start(since: u64) -> Self {
        Self {
            since,
            ids: Vec::new(),
        }
    }

    /// Read a cursor given with `--after`.
    pub fn parse(text: &str) -> Result<Self, InvalidWatch> {
        if text.len() > MAX_CURSOR_LEN {
            return Err(InvalidWatch::After);
        }
        let (since, ids) = match text.split_once(':') {
            Some((since, ids)) => (since, Some(ids)),
            None => (text, None),
        };
        let since = canonical_seconds(since).ok_or(InvalidWatch::After)?;
        let ids = match ids {
            Some(list) => parse_ids(list).ok_or(InvalidWatch::After)?,
            None => Vec::new(),
        };
        Ok(Self { since, ids })
    }

    pub fn since(&self) -> u64 {
        self.since
    }

    pub fn ids(&self) -> &[EventId] {
        &self.ids
    }

    /// The cursor after a call that observed `observed` at `now`: every
    /// record accepted in the call, returned or skipped because this cursor
    /// lists it.
    ///
    /// The time becomes the newest observed outer time (what relays filter
    /// on), clamped to `now`, minus [`CURSOR_MARGIN_SECS`], and it never moves
    /// back. While the envelopes of a chat message all fall before the time
    /// and another envelope of it could still be dated at or after it, the
    /// time is held at that message's newest envelope, so that the next call
    /// observes the message again and skips it. When more than
    /// [`MAX_CURSOR_IDS`] records are at or after the time, it rises to the
    /// smallest second that leaves at most that many, but never past `now`
    /// minus the margin (nor past the old time when that is later): a raise
    /// covers only seconds this call observed that ended more than the margin
    /// ago, whatever times the records claim. The ids are the records at or
    /// after the time; when more than [`MAX_CURSOR_IDS`] still qualify, the
    /// cursor lists node messages first, then the newest chat messages, and a
    /// later call returns the others again. With nothing observed the cursor stays as it is. A record that
    /// reaches the relays after records stamped more than the margin later, or
    /// that falls inside a raised range, is never returned by a later call,
    /// and a chat message inside a raised range is returned again if another
    /// envelope of it, dated after the range, reaches the relays.
    pub fn advance(&self, observed: &[Observed], now: u64) -> Self {
        let mut spans: BTreeMap<EventId, (u64, u64)> = BTreeMap::new();
        let mut mail: BTreeMap<EventId, bool> = BTreeMap::new();
        for seen in observed {
            let span = spans.entry(seen.id).or_insert((seen.time, seen.last));
            *span = (span.0.max(seen.time), span.1.max(seen.last));
            *mail.entry(seen.id).or_default() |= seen.mail;
        }
        let Some(newest) = spans.values().map(|&(time, _)| time).max() else {
            return self.clone();
        };
        let floor = self
            .since
            .max(newest.min(now).saturating_sub(CURSOR_MARGIN_SECS));
        let ceiling = self.since.max(now.saturating_sub(CURSOR_MARGIN_SECS));
        let held = held(floor, &spans).max(self.since);
        let since = raised(held, &spans).min(ceiling);
        let mut kept: Vec<(bool, u64, EventId)> = spans
            .into_iter()
            .filter(|&(_, (time, _))| time >= since)
            .map(|(id, (time, _))| (mail[&id], time, id))
            .collect();
        // Node messages first, as chat dated ahead must not push them out;
        // then newest first, so the oldest are the ones a later call returns.
        kept.sort_unstable_by(|a, b| {
            (b.0, b.1)
                .cmp(&(a.0, a.1))
                .then(a.2.as_bytes().cmp(b.2.as_bytes()))
        });
        kept.truncate(MAX_CURSOR_IDS);
        let mut ids: Vec<EventId> = kept.into_iter().map(|(_, _, id)| id).collect();
        ids.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        Self { since, ids }
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.since)?;
        for (position, id) in self.ids.iter().enumerate() {
            f.write_str(if position == 0 { ":" } else { "," })?;
            f.write_str(&id.to_hex())?;
        }
        Ok(())
    }
}

/// `floor`, or the smallest later second that leaves at most
/// [`MAX_CURSOR_IDS`] records at or after it.
fn raised(floor: u64, spans: &BTreeMap<EventId, (u64, u64)>) -> u64 {
    let mut times: Vec<u64> = spans
        .values()
        .map(|&(time, _)| time)
        .filter(|&time| time >= floor)
        .collect();
    if times.len() <= MAX_CURSOR_IDS {
        return floor;
    }
    times.sort_unstable_by(|a, b| b.cmp(a));
    times[MAX_CURSOR_IDS].saturating_add(1)
}

/// `since`, or the newest outer time of a chat message whose envelopes all
/// fall before it while a later one could still be dated at or after it, and
/// so on from there: the next call then observes that message again and skips
/// it, until no envelope of it can reach the time.
fn held(since: u64, spans: &BTreeMap<EventId, (u64, u64)>) -> u64 {
    let mut newest_first: Vec<(u64, u64)> = spans.values().copied().collect();
    newest_first.sort_unstable_by_key(|&(time, _)| std::cmp::Reverse(time));
    newest_first.into_iter().fold(since, |since, (time, last)| {
        if time < since && last >= since {
            time
        } else {
            since
        }
    })
}

/// Decimal seconds with no sign and no leading zeros, within 64 bits.
fn canonical_seconds(text: &str) -> Option<u64> {
    let digits = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.parse().ok()
}

/// 1 to [`MAX_CURSOR_IDS`] distinct lowercase 64-hex ids separated by commas,
/// returned sorted.
fn parse_ids(list: &str) -> Option<Vec<EventId>> {
    let mut ids = list
        .split(',')
        .map(|hex| {
            let lowercase = hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
            if lowercase {
                EventId::from_hex(hex).ok()
            } else {
                None
            }
        })
        .collect::<Option<Vec<EventId>>>()?;
    ids.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let distinct = ids.windows(2).all(|pair| pair[0] != pair[1]);
    (distinct && ids.len() <= MAX_CURSOR_IDS).then_some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_792_108_800;

    fn id(n: u8) -> EventId {
        EventId::from_byte_array([n; 32])
    }

    /// One of 65,536 distinct ids.
    fn many(n: u16) -> EventId {
        let mut bytes = [0; 32];
        bytes[..2].copy_from_slice(&n.to_be_bytes());
        EventId::from_byte_array(bytes)
    }

    /// Node messages at the given times.
    fn mails(records: &[(u64, EventId)]) -> Vec<Observed> {
        records
            .iter()
            .map(|&(time, id)| Observed::mail(id, time))
            .collect()
    }

    fn parses_back(cursor: &Cursor) {
        let text = cursor.to_string();
        assert_eq!(Cursor::parse(&text).as_ref(), Ok(cursor), "{text}");
    }

    #[test]
    fn a_cursor_round_trips_and_malformed_ones_are_refused() {
        for text in ["0", "1792108800", "18446744073709551615"] {
            let cursor = Cursor::parse(text).unwrap();
            assert_eq!(
                (cursor.to_string(), cursor.ids()),
                (text.to_string(), &[][..])
            );
        }
        let mut ids = [id(9).to_hex(), id(1).to_hex(), id(5).to_hex()];
        let cursor = Cursor::parse(&format!("1792108800:{}", ids.join(","))).unwrap();
        ids.sort();
        let canonical = format!("1792108800:{}", ids.join(","));
        assert_eq!(cursor.to_string(), canonical, "the ids are written sorted");
        assert_eq!(cursor.since(), 1_792_108_800);
        assert_eq!(cursor.ids(), &[id(1), id(5), id(9)]);
        parses_back(&cursor);
        let most: Vec<String> = (0..200).map(|n| many(n).to_hex()).collect();
        let full = Cursor::parse(&format!("7:{}", most.join(","))).unwrap();
        assert_eq!(full.ids().len(), 200);
        parses_back(&full);

        let one = id(10).to_hex();
        let too_many: Vec<String> = (0..201).map(|n| many(n).to_hex()).collect();
        for bad in [
            String::new(),
            format!(":{one}"),
            "-1".into(),
            "+1".into(),
            "01".into(),
            "00".into(),
            "1.5".into(),
            "1e3".into(),
            " 1".into(),
            "1 ".into(),
            "abc".into(),
            "18446744073709551616".into(),
            "1:".into(),
            format!("1:{}", one.to_uppercase()),
            format!("1:{}", &one[..63]),
            format!("1:{one}0"),
            format!("1:{one},{one}"),
            format!("1:{one},"),
            format!("1:,{one}"),
            format!("1:{one}:{}", id(11).to_hex()),
            format!("1:{one} "),
            format!("1:{}", too_many.join(",")),
        ] {
            assert_eq!(Cursor::parse(&bad), Err(InvalidWatch::After), "{bad:?}");
        }
    }

    #[test]
    fn the_next_cursor_keeps_a_minute_of_ids() {
        let start = Cursor::start(NOW - 1_800);
        assert_eq!(start.advance(&[], NOW), start, "nothing observed");

        let observed = [
            (NOW - 1_000, id(1)),
            (NOW - 71, id(2)),
            (NOW - 70, id(3)),
            (NOW - 10, id(4)),
        ];
        let next = start.advance(&mails(&observed), NOW);
        assert_eq!(next.since(), NOW - 70, "the newest minus 60 s");
        assert_eq!(next.ids(), &[id(3), id(4)]);
        parses_back(&next);

        let ahead = Cursor::start(NOW);
        let kept = ahead.advance(&mails(&[(NOW - 500, id(5))]), NOW);
        assert_eq!(
            (kept.since(), kept.ids()),
            (NOW, &[][..]),
            "never moves back"
        );

        let future = start.advance(&mails(&[(NOW + 60, id(6)), (NOW - 59, id(1))]), NOW);
        assert_eq!(future.since(), NOW - 60, "clamped to now");
        assert_eq!(future.ids(), &[id(1), id(6)]);
        parses_back(&future);

        let twice = start.advance(&mails(&[(NOW - 100, id(7)), (NOW - 30, id(7))]), NOW);
        assert_eq!((twice.since(), twice.ids()), (NOW - 90, &[id(7)][..]));
        let listed = next.advance(&mails(&[(NOW - 70, id(3)), (NOW - 5, id(8))]), NOW);
        assert_eq!(listed.since(), NOW - 65);
        assert_eq!(
            listed.ids(),
            &[id(8)],
            "an id below the new since is dropped"
        );
    }

    /// JSON calls from `start`, where `arrivals[k]` reach the relays before
    /// call `k`: each call observes every envelope at or after its cursor's
    /// time and returns the records its cursor does not list. Gives what each
    /// call returned and the last cursor.
    fn calls(start: Cursor, arrivals: &[Vec<Observed>]) -> (Vec<Vec<EventId>>, Cursor) {
        let mut relays: Vec<Observed> = Vec::new();
        let mut cursor = start;
        let mut returned = Vec::new();
        for batch in arrivals {
            relays.extend_from_slice(batch);
            let observed: Vec<Observed> = relays
                .iter()
                .copied()
                .filter(|seen| seen.time >= cursor.since())
                .collect();
            let mut new: Vec<EventId> = observed
                .iter()
                .map(|seen| seen.id)
                .filter(|id| !cursor.ids().contains(id))
                .collect();
            new.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            new.dedup();
            returned.push(new);
            cursor = cursor.advance(&observed, NOW);
        }
        (returned, cursor)
    }

    #[test]
    fn a_chat_message_stays_listed_while_a_later_envelope_could_reach_the_time() {
        let t = NOW - 300;
        let (line, first, second, third) = (id(1), id(2), id(3), id(4));
        let chat = |outer: u64| Observed::chat(line, outer, t);
        let mail = |at: u64, record: EventId| Observed::mail(record, at);
        let (returned, last) = calls(
            Cursor::start(NOW - 1_800),
            &[
                vec![chat(t)],
                vec![mail(t + 70, first)],
                vec![mail(t + 75, second)],
                // Another envelope of the line, dated 60 s after it was written.
                vec![chat(t + 60)],
                vec![mail(t + 200, third)],
            ],
        );
        let once = [vec![line], vec![first], vec![second], vec![], vec![third]];
        assert_eq!(returned, once, "each record once");
        assert_eq!(
            (last.since(), last.ids()),
            (t + 140, &[third][..]),
            "released once no envelope of the line can reach the time"
        );

        let start = Cursor::start(NOW - 1_800);
        let held = start.advance(&[chat(t), mail(t + 70, first)], NOW);
        assert_eq!((held.since(), held.ids()), (t, &[line, first][..]));
        parses_back(&held);
        let later_line = Observed::chat(id(5), t + 50, t + 50);
        let chain = start.advance(&[chat(t), later_line, mail(t + 130, first)], NOW);
        assert_eq!(chain.since(), t, "held again at the line before");
        assert_eq!(chain.ids(), &[line, first, id(5)]);
        let written_earlier = Observed::chat(line, t, t - 60);
        let free = start.advance(&[written_earlier, mail(t + 70, first)], NOW);
        assert_eq!(
            (free.since(), free.ids()),
            (t + 10, &[first][..]),
            "no envelope of a line written 60 s before its envelope reaches t + 10"
        );

        let mut crowded = mails(&(0..250).map(|n| (NOW - 500, many(n))).collect::<Vec<_>>());
        crowded.push(Observed::chat(line, NOW - 570, NOW - 570));
        crowded.push(Observed::chat(id(5), NOW - 540, NOW - 540));
        let raised = start.advance(&crowded, NOW);
        assert_eq!(
            (raised.since(), raised.ids()),
            (NOW - 499, &[][..]),
            "the cap passes lines that would hold the time, so the crowded second is not listed again"
        );
    }

    #[test]
    fn the_cap_raises_the_time_only_over_records_older_than_a_minute() {
        let start = Cursor::start(NOW - 1_800);
        let same =
            |time: u64| -> Vec<(u64, EventId)> { (0..250).map(|n| (time, many(n))).collect() };
        let past = start.advance(&mails(&same(NOW - 500)), NOW);
        assert_eq!(past.since(), NOW - 499, "past the crowded second");
        assert!(past.ids().is_empty());
        parses_back(&past);
        let recent = start.advance(&mails(&same(NOW - 5)), NOW);
        assert_eq!((recent.since(), recent.ids().len()), (NOW - 60, 200));
        parses_back(&recent);

        let spread = |count: u16, age: u64| -> Vec<(u64, EventId)> {
            (0..count)
                .map(|n| (NOW - age - u64::from(n % 60), many(n)))
                .collect()
        };
        let fits = start.advance(&mails(&spread(200, 120)), NOW);
        assert_eq!((fits.since(), fits.ids().len()), (NOW - 180, 200));
        let over = start.advance(&mails(&spread(201, 120)), NOW);
        assert_eq!(over.since(), NOW - 178, "the oldest second is left out");
        assert_eq!(over.ids().len(), 198);
        parses_back(&over);

        let last_minute = start.advance(&mails(&spread(201, 0)), NOW);
        assert_eq!(last_minute.since(), NOW - 60, "not raised");
        assert_eq!(last_minute.ids().len(), 200);
        assert!(
            !last_minute.ids().contains(&many(179)),
            "one record of the oldest second comes back in a later call"
        );
        parses_back(&last_minute);
    }

    #[test]
    fn records_stamped_ahead_never_move_the_time_past_now_minus_the_margin() {
        let start = Cursor::start(NOW - 1_800);
        let flood: Vec<(u64, EventId)> = (0..201).map(|n| (NOW + 60, many(n))).collect();
        let next = start.advance(&mails(&flood), NOW);
        assert_eq!((next.since(), next.ids().len()), (NOW - 60, 200));
        parses_back(&next);

        let mixed: Vec<(u64, EventId)> = (0..201)
            .map(|n| (if n < 150 { NOW + 60 } else { NOW + 30 }, many(n)))
            .collect();
        let next = start.advance(&mails(&mixed), NOW);
        assert_eq!((next.since(), next.ids().len()), (NOW - 60, 200));
        assert!(
            (0..150).all(|n| next.ids().contains(&many(n))),
            "the newest kept"
        );
        assert!(!next.ids().contains(&many(200)));

        let last: Vec<(u64, EventId)> = (0..201).map(|n| (u64::MAX, many(n))).collect();
        let edge = start.advance(&mails(&last), NOW);
        assert_eq!((edge.since(), edge.ids().len()), (NOW - 60, 200));
        parses_back(&edge);

        let mut flood: Vec<Observed> = (0..250)
            .map(|n| Observed::chat(many(n), NOW + 60, NOW + 60))
            .collect();
        flood.extend(mails(&[(NOW - 30, id(1)), (NOW - 59, id(2))]));
        let next = start.advance(&flood, NOW);
        assert_eq!((next.since(), next.ids().len()), (NOW - 60, 200));
        assert!(
            next.ids().contains(&id(1)) && next.ids().contains(&id(2)),
            "chat dated ahead never pushes node messages out of the cursor"
        );

        let ahead = Cursor::start(NOW + 100);
        let later: Vec<(u64, EventId)> = (0..201).map(|n| (NOW + 150, many(n))).collect();
        let kept = ahead.advance(&mails(&later), NOW);
        assert_eq!(
            (kept.since(), kept.ids().len()),
            (NOW + 100, 200),
            "never moves back"
        );
    }
}
