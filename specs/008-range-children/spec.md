# Feature Specification: Range Order Children

**Feature**: `008-range-children`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-02

**Status**: Implemented and verified on Linux against a scripted node and mailbox (slices 1-6, 2026-11-03; slice 7, the index reserved before publishing, 2026-11-05); CI is checked after the push. Probed on the maintainer's local regtest node on 2026-11-03 for a sell range order (see `tests/TEST_SUMMARY.md`); the buy case and an exhausted slice were not. The maintainer approved the shape including `order sync`. Progress is tracked only in [tasks.md](tasks.md)

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
- Q: The index? -> A: Reserved after the confirmation and right before publishing by recording it in the store in one short write transaction
  committed before anything is sent, as `order new` and `order take-*` now do: the reservation is the spend. The node never validates or
  records a `NextTrade` index, so recording it first is what keeps a run that dies after publishing from letting a later order reuse the
  child's key, and it lets `order sync` derive that key. A request that never left, or that the node refused (it refuses before it keeps a key
  or makes a child), gives the index back unless another command reserved one meanwhile (the index is then skipped). `uncertain` outcomes
  report `applied` in `meta.persistence` when a key was sent. The reserved index is `data.next_trade_index` (amended 2026-11-05 by spec 008,
  slice 7: the index was first spent only after the exchange, which a crash or a concurrent run could turn into a reused key).
- Q: What does the outcome say about the child? -> A: `next_step` `sync_child_order` (new) on an accepted `release` of a sell range order, and
  on an accepted `fiat-sent` of a buy range order `next_step` stays `wait_for_release` with the child arriving later; the human text names
  `order sync`. The warning `range_not_continued` of spec 007 is removed (the key is sent whenever it can be); an index that cannot be reserved stops the step with `store_unwritable` before anything is sent (amended 2026-11-05 by spec 008, slice 7: the
  warning `trade_index_not_recorded` of slice 3 is removed, since an index is recorded before publishing).
- Q: Existing rows? -> A: Rows written before this spec carry `is_mine = 1` for takers too. They are treated as the user's own (the
  conservative reading: the key is sent and an index is spent); the node ignores it for a taker. Rows saved from now on are exact. No
  migration.

## Scope

Maker marker, next trade key and index, `order sync`. Out of scope: waiting for the child inside a step, the dispute solver-key save, MCP, range
orders the user took (the taker needs no child), editing a child.

## Requirements

- **FR-001**: `Store::save_order` records whether the user made the order (`order new`: mine; `order take-*`: not). `range_has_room` requires the
  order to be mine. The `range_not_continued` false positive disappears.
- **FR-002**: `fiat-sent` and `release` carry `NextTrade(key, index)` exactly when the clarification says, with the index recorded right before
  publishing and given back only after a refusal or a request that never left (amended 2026-11-05 by spec 008, slice 7); otherwise they stay as
  in spec 007.
- **FR-003**: An accepted step that sent a key reports `next_trade_index` and, for a sell order's `release`, `next_step` `sync_child_order`.
- **FR-004**: `order sync [--since MINUTES]` reads the `new-order` messages of the user's trade keys through `msg::inbox`, and saves a row
  (id, kind, status, amounts, trade key, `is_mine = 1`) for every order id the store does not know, never moving the trade index backwards (a child's index is already recorded, since keys are reserved before publishing; amended 2026-11-05 by spec 008, slice 7). It publishes
  nothing, so it takes no `--yes` and asks no question (amended 2026-11-02 at slice 4: the confirmation rule of the write commands protects an
  irreversible publication; this command only caches rows for the user's own keys and never replaces one). Idempotent: a second run saves nothing.
- **FR-005**: `order sync` data: the orders adopted (id, trade index, status) and the number of messages read, keys read and messages ignored (known
  orders, other actions). Node text is escaped; no key or invoice is serialized.
- **FR-006**: Statuses and codes as for `message list` (reads) and `order new` (the local write); a failed save of one order is reported and the
  others are still saved. Canonical name `order.sync`, one versioned JSON outcome, schema, fixtures, examples, drift checks.
- **FR-007**: The documents of spec 007 are amended where they say a range is not continued; the warning `range_not_continued` is removed.

## Success Criteria

- **SC-001**: Against the scripted node, a sell range order's `release` publishes one request with a next trade key at the next index, the
  scripted node returns the child's `new-order` to the new key, and `order sync` saves the child, after which `order cancel` and the other steps
  work on it. The same for a buy range order's `fiat-sent`, with the child arriving at the peer's release.
- **SC-002**: A taker's `fiat-sent`/`release`, a fixed order, an exhausted range and `cancel` send no key and spend no index.
- **SC-003**: `order sync` writes only rows that did not exist, is idempotent, never publishes, and leaves the database byte-identical when there is
  nothing to adopt or on any failure before the first save.
- **SC-004**: Human and JSON agree; no secret sentinel appears in a diagnostic stream; Linux and the CI Windows runner pass.
- **SC-005**: Nothing about a real node is claimed until a separately authorized regtest probe of a range order is recorded.
- **SC-006**: A run killed after publishing a continuation leaves its index recorded: the next order uses a fresh key and `order sync` adopts
  the child. A run of `order new` killed after publishing never lets the next one reuse its key. Concurrent reservations never share an index.
  Every writer of the index only raises it (the legacy `getlasttradeindex` and order save too; a future migrated `restore` must as well)
  (added 2026-11-05, slice 7).

## Assumptions And Open Items For The Plan

- Whether a real node (regtest v0.19.3) behaves as the v0.20.1 source says for a buy range order's `fiat-sent` key, and whether it ignores a key sent
  by a taker, needs the probe.
- The message of the child carries the order's terms and status; the plan checks it is enough for the row (`save_in` needs id, kind and status).
- Name of the command (`order sync`) and the `--since` window default (the plan proposes 7 days, the `message list` maximum) are open to change.
