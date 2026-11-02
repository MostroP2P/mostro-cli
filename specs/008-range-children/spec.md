# Feature Specification: Range Order Children

**Feature**: `008-range-children`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-02

**Status**: In progress (slice 1 done, 2026-11-02); the maintainer approved the shape including `order sync`. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Required follow-up of [trade steps](../007-trade-steps-json/spec.md): continue a range order from the CLI. A maker's range order
(`order new --fiat-amount 10 --fiat-amount-max 100`) is taken one slice at a time; after each slice the node creates a child order for the rest, but
only if the maker's request carried a fresh trade key. Spec 007 sends none and warns `range_not_continued`; this spec sends it, keeps the index and
gives the child order a usable local row. None of the earlier documents change except where the clarifications below say so.

## Findings (from the node source, v0.20.1, and the maintainer's regtest probes)

- **Who sends the key, and when.** If the maker sells (a `sell` range order), the next trade key travels with the maker's `release`. If the maker
  buys (a `buy` range order), it travels with the maker's `fiat-sent`; the node stores it and uses it when the seller releases. The node ignores a
  key sent by anyone else or at another step, but the legacy CLI sent one in both steps.
- **What the node does.** It creates the child order (same terms, the remaining amount, status `pending`) with the maker's next key as its trade
  key, and sends the maker a `new-order` message with the child as payload, addressed to that **new** key, carrying the request id of the
  triggering request. For a buy range order the message arrives when the seller releases, not when the maker sends `fiat-sent`.
- **No local row for the child.** Commands that act on an order read its trade key from the local `orders` table (`order_unknown` otherwise).
  The legacy CLI created the row when it saw the `new-order` message. `message list` is read-only and shows the message, but saves nothing.
- **The maker marker is wrong today.** `Store::save_order` writes `is_mine = 1` for every order it saves, including the orders taken with
  `order take-*`, so `is_mine` cannot tell the maker from the taker. Spec 007's `range_not_continued` warning therefore also appears for the
  taker of a range order (a false positive found while writing this spec; the legacy `is_mine` test had the same flaw).

## Clarifications

### Session 2026-11-02 (defaults, open to veto)

- Q: How does the child order get its local row? -> A: A new write command, `order sync`, that reads the node's `new-order` messages for the
  user's trade keys (the inbox read of `message list`) and saves a row for each order the store does not know, with its trade key and index, in one
  transaction per order. It covers both cases (a sell range order, where the child arrives right after `release`, and a buy range order, where it
  arrives when the peer releases) with no waiting inside `fiat-sent` and `release`, and it recovers any child that was missed. Alternative
  rejected: waiting inside `release` for the child (the legacy way) only helps one of the two cases and adds a second correlation on another key.
- Q: Is `order sync` read-only? -> A: No: it is the one command that writes rows from the node's messages, and only rows (never an existing
  one). Nothing else changes: `message list` stays read-only. Related and not decided here: the solver-key save for disputes could later be a
  second thing `order sync` adopts.
- Q: When is the key sent? -> A: Only when the user is the maker of a range order with room left (`max - fiat_amount >= min` on the local row) and
  the step is the maker's: `release` for a sell order, `fiat-sent` for a buy order. Never for a taker, a fixed order or `cancel`.
- Q: The index? -> A: Reserved right before publishing and spent once the request was published and the node did not provably refuse it, as in
  `order new`; a refusal or a request that never left spends nothing. `uncertain` outcomes report the stage of that record in `meta.persistence`
  (`Stages::Step` already allows it). The reserved index is `data.next_trade_index`.
- Q: What does the outcome say about the child? -> A: `next_step` `sync_child_order` (new) on an accepted `release` of a sell range order, and
  on an accepted `fiat-sent` of a buy range order `next_step` stays `wait_for_release` with the child arriving later; the human text names
  `order sync`. `range_not_continued` is no longer emitted when the key was sent.
- Q: Existing rows? -> A: Rows written before this spec carry `is_mine = 1` for takers too. They are treated as the user's own (the
  conservative reading: the key is sent and an index is spent); the node ignores it for a taker. Rows saved from now on are exact. No
  migration.

## Scope

Maker marker, next trade key and index, `order sync`. Out of scope: waiting for the child inside a step, the dispute solver-key save, MCP, range
orders the user took (the taker needs no child), editing a child.

## Requirements

- **FR-001**: `Store::save_order` records whether the user made the order (`order new`: mine; `order take-*`: not). `range_has_room` requires the
  order to be mine. The `range_not_continued` false positive disappears.
- **FR-002**: `fiat-sent` and `release` carry `NextTrade(key, index)` exactly when the clarification says, with the index reserved right before
  publishing and kept per the index policy; otherwise they stay as in spec 007.
- **FR-003**: An accepted step that sent a key reports `next_trade_index` and, for a sell order's `release`, `next_step` `sync_child_order`.
- **FR-004**: `order sync [--since MINUTES] [--yes]` reads the `new-order` messages of the user's trade keys through `msg::inbox`, and saves a row
  (id, kind, status, amounts, trade key, `is_mine = 1`) for every order id the store does not know, advancing the trade index if needed. JSON needs
  `--yes` (it writes); human mode lists what it would save and asks. It publishes nothing. Idempotent: a second run saves nothing.
- **FR-005**: `order sync` data: the orders adopted (id, trade index, status) and the number of messages read, keys read and messages ignored (known
  orders, other actions). Node text is escaped; no key or invoice is serialized.
- **FR-006**: Statuses and codes as for `message list` (reads) and `order new` (the local write); a failed save of one order is reported and the
  others are still saved. Canonical name `order.sync`, one versioned JSON outcome, schema, fixtures, examples, drift checks.
- **FR-007**: The documents of spec 007 are amended where they say a range is not continued: the warning remains only for rows that are not exact.

## Success Criteria

- **SC-001**: Against the scripted node, a sell range order's `release` publishes one request with a next trade key at the next index, the
  scripted node returns the child's `new-order` to the new key, and `order sync` saves the child, after which `order cancel` and the other steps
  work on it. The same for a buy range order's `fiat-sent`, with the child arriving at the peer's release.
- **SC-002**: A taker's `fiat-sent`/`release`, a fixed order, an exhausted range and `cancel` send no key and spend no index.
- **SC-003**: `order sync` writes only rows that did not exist, is idempotent, never publishes, and leaves the database byte-identical when there is
  nothing to adopt or on any failure before the first save.
- **SC-004**: Human and JSON agree; no secret sentinel appears in a diagnostic stream; Linux and the CI Windows runner pass.
- **SC-005**: Nothing about a real node is claimed until a separately authorized regtest probe of a range order is recorded.

## Assumptions And Open Items For The Plan

- Whether a real node (regtest v0.19.3) behaves as the v0.20.1 source says for a buy range order's `fiat-sent` key, and whether it ignores a key sent
  by a taker, needs the probe.
- The message of the child carries the order's terms and status; the plan checks it is enough for the row (`save_in` needs id, kind and status).
- Name of the command (`order sync`) and the `--since` window default (the plan proposes 7 days, the `message list` maximum) are open to change.
