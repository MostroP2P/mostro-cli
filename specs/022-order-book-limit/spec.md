# Feature Specification: A Complete Order Book In `order list`

**Feature**: `022-order-book-limit`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-10

**Status**: Unit 1 (FR-001, FR-002, FR-006) implemented and verified on Linux against loopback relays; unit 2 in progress. Progress is tracked only in [tasks.md](tasks.md).

**Input**: an agent using `order list` (through the MCP server) reported 27 pending orders (11 buy, 16 sell) while the node had 33
(14 buy, 19 sell). It could only guess at the cause from `meta.snapshot.received_events: 50`.

## Cause (source reading, `src/msg/orders.rs`)

`order list` sends one filter to every relay: kind 38383, author the node, `#z: order`, `since` seven days ago and `limit: 50`. It does
not filter by status. Kind 38383 is addressable, so a relay keeps one event per order (its latest state), and the 50 events are the 50
orders updated most recently, in any status. The status filter (`pending` by default) runs afterwards, on the client, in
`reduce_orders`. With more than 50 orders updated in the window, the older pending orders are cut off, and nothing in the outcome
says so: `complete_history` is always `false`, and there is no `truncated` field like `message.list` has.

## Decisions

- **D1, status filter at the relay.** The query carries `#s: <status>`, the tag value the node publishes for the requested status
  (`Status` in kebab case: `pending`, `in-progress`, ...). `order list` always has a status (`pending` when none is given), so the tag
  is always sent. The latest-state reduction and the client-side status filter stay, so two relays that disagree still resolve to the
  newest event and a relay that ignores `#s` cannot add other statuses.
- **D2, a larger limit for the order book.** `order list` asks for 300 events per relay (`ORDER_BOOK_LIMIT`). The shared
  `PER_RELAY_LIMIT` of 50 stays for `message list`, `order chat`, `order sync` and `order watch`, which this spec does not change.
- **D3, a truncation flag.** `data.truncated` is `true` when at least one relay delivered `ORDER_BOOK_LIMIT` matching events before its
  EOSE, so older orders in the requested status may be missing. Human mode prints a warning line on stderr in that case.
- **D4, versioning.** The JSON changes stay in `schema_version` 1 under a documented exception, as spec 021 did:
  `order.list` now pins `meta.snapshot.per_relay_limit` to 300 (50 stays for the other commands), and `data.truncated` is a new
  required boolean. Both go in `docs/commands.md` (Versioning).

## Accepted Risk

The relays are asked only for events in the requested status. When an order's newer state in another status is missing from one
relay (it missed the update) or a relay keeps superseded versions of an addressable event (against NIP-01), that relay returns the
order's older `pending` state, and the relays holding the newer `canceled` or `in-progress` state do not return it. The order is then
listed although it left the status. Before this spec the latest-state reduction across relays caught that case
(`listing_selects_latest_matching_states_across_relays` now asserts the new behaviour). The risk is accepted: the node publishes each
state to all its relays, and the alternative (no relay-side filter) is the defect this spec fixes. A taker of such an order gets the
node's refusal. `docs/commands.md` states it.

## User Scenarios & Testing

### Story 1: every pending order within the limit (P1)

**Given** a node with 60 orders updated in the last seven days, 33 of them pending (14 buy, 19 sell) and the other 27 the most
recently updated, **when** a client runs `order list --output json`, **then** it receives the 33 pending orders and
`data.truncated` is `false`.

### Story 2: a truncated book is reported (P1)

**Given** a relay holding more pending orders of the node than `ORDER_BOOK_LIMIT`, **when** a client lists them, **then**
`data.truncated` is `true` and, in human mode, stderr carries one warning line; the orders received are still listed.

### Story 3: other statuses (P2)

**Given** orders in several statuses, **when** a client runs `order list --status canceled`, **then** the relay is asked for
`#s: canceled` only, and the result holds only canceled orders.

## Requirements

- **FR-001**: The `order list` filter carries `#s` set to the requested status, in the node's tag spelling.
- **FR-002**: The `order list` filter asks each relay for at most `ORDER_BOOK_LIMIT` (300) events; `meta.snapshot.per_relay_limit`
  reports 300 for `order.list` and 50 for every other command.
- **FR-003**: `fetch_msg` counts the matching events each relay delivered before its EOSE (duplicates across relays count on each)
  and reports whether any relay reached the filter's limit.
- **FR-004**: `order.list` success carries `data.truncated` (boolean, required). An error outcome has no `data`, as today.
- **FR-005**: Human `order list` prints, when truncated, one stderr line:
  `warning: a relay returned its limit of 300 orders; older orders may be missing`.
- **FR-006**: The other users of `seven_day_filter` (`src/util/events.rs`, the human dispute listing) keep their filter.
- **FR-007**: `docs/commands.md` (listing semantics, Versioning), `docs/architecture.md`, the generated schemas and the spec 001
  pointers describe the new behaviour.

## Success Criteria

- **SC-001**: Against loopback relays holding the Story 1 book, `order list --output json` lists 33 with `truncated: false`
  (today it lists fewer).
- **SC-002**: Against a relay holding 301 pending orders, the result holds 300 with `truncated: true`.
- **SC-003**: A scripted relay that ignores `#s` (returning every status) still yields only orders whose latest state matches.
- **SC-004**: All constitution gates pass; the checked-in schemas match the generated ones.
- **SC-005** (manual, optional, read-only): `order list` against the production node's relays reports the same buy and sell pending
  counts as an independent read of those relays on the day of the probe.

## Out Of Scope

- Pagination with `until` (D1 and D2 cover the current book size; D3 reports when it is no longer enough).
- The limits of `message list`, `order chat`, `order sync` and `order watch`.
- The MCP server repository and the mostro-cli book: they quote the 50-event limit and need a follow-up change there.
