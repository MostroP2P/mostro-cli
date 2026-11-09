//! `order.watch`: follow one order the user holds (spec 020). The binary
//! reaches it through `src/cli/order_watch.rs`.
//!
//! The order's trade key and the user's side come from the store, the
//! counterparty from [`find_counterparty`] (the row, else the node's post-lock
//! mail; `counterparty_unknown` only leaves the chat out), then one live
//! [`Watch`] replays the window or the cursor and follows the node's mail and,
//! once the counterparty is known, the chat. A counterparty that a new node
//! message names while watching brings the chat in, its replay in the same
//! update; that key is not saved (the next call's lookup caches it).
//!
//! A JSON call ([`Follow::FirstChange`]) returns at the first new records, or
//! empty at its deadline. A human watch ([`Follow::UntilEnd`]) hands every
//! update to its sink, the first even when empty, until the trade ends, the
//! deadline passes, the sink cannot write or its reader is gone. A completed
//! trade ends at the `rate` the node sends right after its end: an update
//! that holds the end waits for it, up to [`WatchLimits::rate_grace`] and
//! never past the deadline. Both get the same outcome: `ok`
//! with the last update, whose cursor covers every record observed in the
//! call. Nothing is published and nothing is written but the lookup's cache,
//! so every stage is `not_applicable`.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use chrono::{DateTime, Utc};
use mostro_core::prelude::{Action, Kind};
use nostr_sdk::prelude::{Keys, PublicKey, Timestamp};
use tokio::time::Instant;
use uuid::Uuid;

use super::counterparty::find_counterparty;
use crate::commands::message::list::{order_key, record, store_code};
use crate::msg::counterparty::counterparty_in;
use crate::msg::inbox::Mail;
use crate::msg::watch::{Batch, Seen, Watch, WatchLimits};
use crate::msg::PublicReadConfig;
use crate::outcome::chat::{ChatRecord, ChatSender};
use crate::outcome::messages::TradeRole;
use crate::outcome::watch::{ends_trade, rate_follows, Cursor, WatchData, WatchInput, COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings};
use crate::store::Store;

/// The outcome of `order watch`; it never carries a snapshot.
pub type WatchOutcome = Envelope<WatchData, ()>;

/// How long a watch goes on.
pub enum Follow<'a> {
    /// Until the first new records after the replays, or empty at the
    /// deadline: one JSON document.
    FirstChange,
    /// Until the trade ends or the deadline, handing every update to the
    /// sink, the first even when empty. The sink returns `false` when it
    /// could not write the update, which stops the watch; so does the
    /// [`Gone`] future, once the first update is written.
    UntilEnd(&'a mut dyn FnMut(&WatchData) -> bool, Gone<'a>),
}

/// Resolves when the sink's reader went away while the watch waits, so an
/// idle watch stops without waiting for its next write.
pub type Gone<'a> = Pin<&'a mut dyn Future<Output = ()>>;

/// The outcome, and the action that ended the trade, when one did.
pub struct Watched {
    pub outcome: WatchOutcome,
    /// The wire name of the action of the node message that ended the trade.
    pub ended_by: Option<String>,
}

/// The order watched and what the run gathered.
struct Run {
    order: Uuid,
    index: Option<i64>,
    trade: Keys,
    role: Option<TradeRole>,
    peer: Option<PublicKey>,
    start: Cursor,
    /// How long an update that ends a completed trade waits for its `rate`.
    rate_grace: Duration,
    warnings: Warnings,
}

/// Watch `input`'s order as `follow` says. The deadline, when `input` has
/// one, counts from the end of the opening replay.
///
/// Store failures are `order_unknown`, `identity_missing` or
/// `store_unreadable`, before any relay is contacted; a failed lookup keeps
/// its code, except `counterparty_unknown`, which starts the watch without the
/// chat; a watch that does not open keeps the read codes; more records than a
/// run allows is `read_limit_exceeded`.
pub async fn order_watch(
    input: &WatchInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &WatchLimits,
    now: DateTime<Utc>,
    follow: Follow<'_>,
) -> Watched {
    let mut run = match prepare(input, config, store, limits, now).await {
        Ok(run) => run,
        Err((code, warnings)) => return failed(code, warnings),
    };
    let trade = (run.index, &run.trade);
    let opened = Watch::open(config, trade, run.peer.as_ref(), &run.start, limits).await;
    let mut watch = match opened {
        Ok(watch) => watch,
        Err(failure) => {
            run.warnings.merge(failure.warnings());
            return failed(failure.code(), run.warnings);
        }
    };
    let deadline = input
        .timeout_secs()
        .map(|secs| Instant::now() + Duration::from_secs(secs));
    let result = follow_until(&mut run, &mut watch, deadline, follow).await;
    let closed = watch.close().await;
    run.warnings.merge(&closed);
    match result {
        Ok((data, ended_by)) => Watched {
            outcome: Envelope::ok(COMMAND, data, run.warnings, ExecutionMeta::read(None)),
            ended_by,
        },
        Err(code) => failed(code, run.warnings),
    }
}

/// The order's key, the user's side and the counterparty, if known.
async fn prepare(
    input: &WatchInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &WatchLimits,
    now: DateTime<Utc>,
) -> Result<Run, (ErrorCode, Warnings)> {
    let order = input.order();
    let alone = |code| (code, Warnings::new());
    let (index, trade) = order_key(store, order).await.map_err(alone)?;
    let role = order_side(store, order).await.map_err(alone)?;
    let seconds = u64::try_from(now.timestamp()).unwrap_or(0);
    let found = find_counterparty(order, &trade, config, store, &limits.read, seconds.into());
    let (peer, warnings) = match found.await {
        Ok(found) => (Some(found.key()), found.warnings().clone()),
        Err(failure) if failure.code() == ErrorCode::CounterpartyUnknown => {
            (None, failure.warnings().clone())
        }
        Err(failure) => return Err((failure.code(), failure.warnings().clone())),
    };
    Ok(Run {
        order,
        index,
        trade,
        role,
        peer,
        start: input.start_cursor(seconds),
        rate_grace: limits.rate_grace,
        warnings,
    })
}

/// The user's side in `order`, from its row; `None` when the row does not
/// say (`is_mine` NULL, or a kind that does not parse).
async fn order_side(store: &Store, order: Uuid) -> Result<Option<TradeRole>, ErrorCode> {
    let rows = store.order_rows().await.map_err(store_code)?;
    let id = order.to_string();
    Ok(rows.iter().find(|row| row.id == id).and_then(|row| {
        let kind = row.kind.parse::<Kind>().ok()?;
        Some(TradeRole::of_order(kind, row.is_mine?))
    }))
}

/// The updates, until `follow` is done: the last one and how the trade ended.
async fn follow_until(
    run: &mut Run,
    watch: &mut Watch,
    deadline: Option<Instant>,
    follow: Follow<'_>,
) -> Result<(WatchData, Option<String>), ErrorCode> {
    let Follow::UntilEnd(sink, mut gone) = follow else {
        let batch = next_update(run, watch, deadline).await?;
        let batch = with_rate(run, watch, batch, deadline).await?;
        return Ok(run.update(watch, batch));
    };
    // The first update at once: the replay, even when it is empty.
    let mut wait = Some(Instant::now());
    // The last update written; `None` until the first one is.
    let mut last = None;
    loop {
        let first = last.is_none();
        let batch = match last.take() {
            None => next_update(run, watch, wait).await?,
            Some(written) => tokio::select! {
                biased;
                () = &mut gone => return Ok(written),
                batch = next_update(run, watch, wait) => batch?,
            },
        };
        let batch = with_rate(run, watch, batch, deadline).await?;
        let empty = batch.items.is_empty();
        let (data, ended_by) = run.update(watch, batch);
        let could_not_write = (first || !empty) && !sink(&data);
        // After the first update, an empty batch means the deadline passed.
        if could_not_write || data.ended() || (empty && !first) {
            return Ok((data, ended_by));
        }
        last = Some((data, ended_by));
        wait = deadline;
    }
}

/// The next batch. When it names the counterparty for the first time, the
/// chat is followed and its replay joins the batch.
async fn next_update(
    run: &mut Run,
    watch: &mut Watch,
    deadline: Option<Instant>,
) -> Result<Batch, ErrorCode> {
    let mut batch = watch.next(deadline).await?;
    if run.peer.is_some() {
        return Ok(batch);
    }
    let mail: Vec<Mail> = batch
        .items
        .iter()
        .filter_map(|seen| match seen {
            Seen::Mail(mail) => Some(mail.as_ref().clone()),
            Seen::Chat { .. } => None,
        })
        .collect();
    if let Some(peer) = counterparty_in(&mail, run.order, &run.trade.public_key()) {
        watch.follow_chat(&peer).await?;
        run.peer = Some(peer);
        let replay = watch.next(Some(Instant::now())).await?;
        batch.items.extend(replay.items);
        batch.truncated |= replay.truncated;
    }
    Ok(batch)
}

/// `batch` and, when it ends a completed trade without its `rate`, the
/// records that follow, until the `rate` arrives, the grace window passes or
/// the deadline does.
async fn with_rate(
    run: &mut Run,
    watch: &mut Watch,
    mut batch: Batch,
    deadline: Option<Instant>,
) -> Result<Batch, ErrorCode> {
    let grace = Instant::now() + run.rate_grace;
    let until = deadline.map_or(grace, |deadline| deadline.min(grace));
    while run.awaits_rate(&batch) && Instant::now() < until {
        let more = next_update(run, watch, Some(until)).await?;
        if !absorb(&mut batch, more) {
            break;
        }
    }
    Ok(batch)
}

/// Add `more` to `batch`; `false` when `more` was empty (the wait ended).
fn absorb(batch: &mut Batch, more: Batch) -> bool {
    if more.items.is_empty() {
        return false;
    }
    batch.items.extend(more.items);
    batch.truncated |= more.truncated;
    true
}

impl Run {
    /// `batch` as an update, with the cursor of everything observed so far,
    /// and the action that ended the trade, if one did.
    fn update(&mut self, watch: &Watch, mut batch: Batch) -> (WatchData, Option<String>) {
        // A `rate` follows the end of the trade stamped in the same second.
        batch
            .items
            .sort_by_key(|seen| (seen.created_at(), is_rate(seen), seen.id()));
        let ours = self.trade.public_key();
        let (mut messages, mut chat, mut ended_by) = (Vec::new(), Vec::new(), None);
        // The end of a completed trade names it rather than its `rate`.
        let mut by_rate = false;
        for seen in &batch.items {
            match seen {
                Seen::Mail(mail) => {
                    let message = record(mail, &mut self.warnings);
                    if (ended_by.is_none() || by_rate) && self.ends(mail) {
                        by_rate = mail.message.get_inner_message_kind().action == Action::Rate;
                        ended_by = Some(message.action().to_string());
                    }
                    messages.push(message);
                }
                Seen::Chat { line, .. } => {
                    let from = ChatSender::of(&line.sender, &ours);
                    let (id, at) = (&line.message_id, line.created_at);
                    let text = &line.text;
                    chat.push(ChatRecord::new(id, from, at, text, &mut self.warnings));
                }
            }
        }
        let cursor = self
            .start
            .advance(&watch.observed(), Timestamp::now().as_secs());
        let records = (messages, chat);
        let ended = ended_by.is_some();
        let peer = self.peer.as_ref();
        let data = WatchData::new(self.order, peer, records, ended, batch.truncated, &cursor);
        (data, ended_by)
    }

    /// Whether `batch` ends a completed trade but lacks the `rate` that the
    /// node sends right after.
    fn awaits_rate(&self, batch: &Batch) -> bool {
        let actions: Vec<&Action> = batch
            .items
            .iter()
            .filter_map(|seen| match seen {
                Seen::Mail(mail) => {
                    let kind = mail.message.get_inner_message_kind();
                    (kind.id == Some(self.order)).then_some(&kind.action)
                }
                Seen::Chat { .. } => None,
            })
            .collect();
        actions.iter().any(|action| rate_follows(action)) && !actions.contains(&&Action::Rate)
    }

    /// Whether `mail` is about this order and ends its trade for the user.
    fn ends(&self, mail: &Mail) -> bool {
        let kind = mail.message.get_inner_message_kind();
        kind.id == Some(self.order) && ends_trade(&kind.action, self.role)
    }
}

/// Whether `seen` is a node's `rate`.
fn is_rate(seen: &Seen) -> bool {
    matches!(seen, Seen::Mail(mail) if mail.message.get_inner_message_kind().action == Action::Rate)
}

fn failed(code: ErrorCode, warnings: Warnings) -> Watched {
    Watched {
        outcome: Envelope::error(Some(COMMAND), code, warnings, ExecutionMeta::read(None)),
        ended_by: None,
    }
}
