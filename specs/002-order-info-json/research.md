# Research: Order Info

**Date**: 2026-10-28 | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Each section records a decision, why, and what was rejected. Evidence comes from the
code in this repository, the pinned `mostro-core` 0.17.3, the workspace checkout of
the daemon (`mostro`, tag v0.20.1 plus three commits, `b94e3f3f`), and the protocol
page ([orders](https://mostro.network/protocol/orders.html), external documentation,
read 2026-10-28). Nothing here was observed against a running daemon; the regtest
probe in the plan is where that happens.

## What the daemon does for `orders` (evidence)

`mostro/src/app/orders.rs`, `orders_action`:

| Request | Daemon answer |
| --- | --- |
| payload is not `Ids`, or the list is empty | `CantDo(invalid_parameters)` |
| more ids than `max_orders_per_response` (template default 10) | `CantDo(too_many_requests)` for the whole request; no truncation |
| ids given, none is the caller's | `CantDo(not_found)` |
| ids given, some are the caller's | `Orders` payload with only the caller's matches; other ids are left out |

The reply is queued to the sender with the request's `request_id`; the returned orders have
`buyer_invoice` cleared. Authorization uses the identity the daemon derives from the request,
so only the requesting identity's own orders can ever come back. The daemon publishes
`max_orders_per_response` as a tag of its kind-38385 info event (`nip33.rs`).

Differences from the protocol page: the page says an over-limit request is truncated to the
first allowed ids, and does not mention `not_found`. The MCP prototype's regtest note
(*Resource not found* for another user's order, daemon v0.19.3) matches the `not_found`
row. The v0.19.3 source was not available, so the table is evidence for v0.20.1 only.

## D1. Read-only identity loading in a new module

**Decision**: add `src/identity.rs`: resolve the data directory the way `get_mcli_path` does
but **without creating or chmod-ing it**, open `mcli.db` with SQLite read-only and
no-create options, read the `users` row, derive the identity key with the same derivation as
`User::get_identity_keys`, and return typed errors `Missing` (no directory, file or row) and
`Unreadable` (anything else). `db.rs` then delegates its derivation to the shared function.
No logger, no `set_var`, no migration, no permission change.

**Why**: `db::connect()` creates the database and a mnemonic, re-tightens permissions and runs a
migration; `get_mcli_path` creates the directory and panics on failure; `init_context` also calls
`get_next_trade_keys`. Each is an effect the spec forbids for a read. `db.rs` is frozen
(704 production lines) so it cannot take new code.

**Rejected**: a flag on `connect()` (touches a frozen file and keeps a create path next to the
read path); reading the mnemonic file directly (the store is SQLite); a new table or
cache (no new storage model).

## D2. One public `msg::exchange`, with private send and receive doors

**Decision**: the wrapper calls `msg::exchange(config, outbound, expect, limits)`. Inside
`msg`, `send_msg.rs` (wrap and publish) and a new `reply.rs` (correlated reply) are private
doors that share the private `Session`. `exchange` connects once, probes the node info,
arms the subscription, publishes, waits, and cleans up in bounded time.

**Why**: the client must stay private to `msg`, and subscribe-before-send needs both halves on
one connection, so a wrapper cannot orchestrate them from outside. The roadmap's separate
`send_msg` and `receive_msg` functions return when a flow needs several steps on one session
(the range-order release); that is not this feature. `receive_msg.rs` (249 lines, the public
read) stays as it is; correlated replies go in `reply.rs` to respect the 400-line limit.

**Rejected**: exporting a session handle (breaks the privacy proved by the doctests); extending
`fetch_msg` (it is a bounded read with a snapshot, not a request/reply); calling the legacy
`wait_for_dm_matching` (needs the legacy `Context`).

## D3. Reuse the existing correlation and decoding

**Decision**: use `util::reply::select_reply_event` and `ExpectedReply` (pure decisions: Mostro-authored
kind 14, tagged to our key, not our own echo, same `request_id`, allowed action) and
`mostro_core::transport::wrap_message_with` and `unwrap_incoming` for the wire form.

**Why**: they are tested, small and already the project's single implementation of the rule. T007
fixed the unrelated-DM race with them. `parse_dm_events` is not used: it prints warnings to
stderr, which a migrated path must not.

**Rejected**: a second copy of the correlation rules; moving `util/reply.rs` (churn without value).

## D4. Transport, proof of work and node info in one bounded probe

**Decision**: inside `exchange`, read the node's kind-38385 info event once (3-second budget,
best effort), then: an explicit `--transport` or `TRANSPORT` value wins (only `nip44` is valid);
otherwise `protocol_version` 1 is `protocol_unsupported`, and 2, other or absent mean `nip44`. If the
advertised `pow` is above the configured `--pow` or `POW`, fail before publishing as `pow_required`.
After a timeout, probe again and also compare `pow_first_contact`; a shortfall then is
`pow_required` with the request already published.

**Why**: this is the legacy behavior (`resolve_transport`, `ensure_first_contact_pow`, the post-timeout
probe), without `set_var` or a logger. A daemon silently drops under-mined events, so the pre-check
saves a 15-second wait.

**Rejected**: mining PoW automatically (not requested); treating a missing info event as an error
(legacy defaults to `nip44`); failing on `pow_first_contact` before publishing (the identity key
may not be first contact; the daemon would simply answer).

## D5. Outcome shape and the stage vocabulary

**Decision**: `order.info` is a new command in contract v1. Its meta uses the existing stage fields
with a request vocabulary: nothing sent is `not_attempted / unknown / not_applicable`; a reply is
`published / accepted / not_applicable`; a refusal is `published / rejected / not_applicable`; a
timeout or a read failure after publishing is `published / unknown / not_applicable`. Persistence
is `not_applicable` because nothing is written. `meta.snapshot` is `null`. No outcome is `uncertain`.

**Why**: the envelope already carries these enums (the contract reserved them), so the keys of
`order.list` and `schema` do not change. An agent that sees a timeout learns whether the node
may have the request.

**Rejected**: a new `meta` key for exchange evidence (changes every existing document); reusing the
`snapshot` key for something that is not a snapshot.

## D6. New codes are command-scoped, not new envelope fields

**Decision**: add error codes `identity_missing`, `identity_unreadable`, `protocol_unsupported`,
`pow_required`, `request_rejected`, `reply_timeout`, `node_refused` and `reply_malformed`, and warnings
`orders_not_returned` and `invalid_order_timestamp`. `node_refused` carries an optional
`error.reason`, the node's refusal reason in snake case (a string matching `^[a-z][a-z0-9_]*$`, because the
reason set belongs to `mostro-core` and grows). The generator lists in each command's schema only
the frozen pilot codes plus that command's own, so the `order.list` and `schema` documents and fixtures
stay byte-for-byte equivalent after parsing. The `reason` property exists only in the failure branch of
commands that declare it.

**Why**: the contract forbids changing an enum's meaning in version 1, not adding a command with its own
codes. Per-command schemas already restrict `error.code` (the `schema` command has two), so this extends
the same rule to the shared `$defs`.

**Rejected**: a rate-limit code separate from `node_refused` (the daemon uses one reason for a rate limit and for
too many ids); putting the reason in the message (messages are fixed by code); a new schema version.

## D7. `CantDo(not_found)` becomes the empty success

**Decision**: map the daemon's `not_found` answer to `ok` with no orders and every id in `not_returned`, plus the warning.
Every other `CantDo` is `node_refused`.

**Why**: the daemon uses `not_found` for "none of these is yours", which is the same fact as a partial reply
that omits ids. Without this, one question would have two answers depending on how many ids matched.

**Rejected**: surfacing `not_found` as an error (inconsistent with the partial case); inventing a distinction
between unknown and foreign ids (the node gives none, and revealing it would not be possible).

## D8. No client-side id cap, no splitting

**Decision**: as decided in the spec. The command sends every distinct id it is given in one request; the
daemon's refusal (`too_many_requests`) is reported as `node_refused` with that reason. The info event's
advertised `max_orders_per_response` is read for no purpose in this feature; surfacing it in the refusal is a possible
later change, noted but not planned.

**Why**: the maintainer's decision; the daemon is the authority and its refusal is cheap.

## D9. Fixture strategy: a scripted fake daemon on the loopback relay

**Decision**: a shared `tests/common/fake_mostro.rs` connects a client with the node's keys to the existing loopback
relay, decrypts each request, records it, and replies per script (orders, subset, extras, duplicates, each refusal,
silence, unrelated or wrong-id or wrong-author messages first, malformed payload, delayed reply). A seeded
kind-38385 event provides `protocol_version`, `pow`, `pow_first_contact` and `max_orders_per_response`. Identity
fixtures create a real `mcli.db` with `sqlx` in the test (generated mnemonic), so byte-identical checks cover a real file.

**Why**: the pilot's `LocalRelay` harness already runs children against generated keys. A fake daemon needs only the
public wire API (`wrap_message_with`, `unwrap_incoming`).

**Rejected**: a mock at the `Client` level (would bypass the code under test); real `mostrod` for every test (slow, regtest only).

## D10. Human rendering is pure and equivalent, not byte-identical

**Decision**: `parse_human/order_info.rs` renders the typed outcome with the familiar labelled lines of the legacy
"Orders List" (id, kind, status, amount, currency, fiat amount or range, payment method, premium, created, expires) and adds a
"not returned" line. The legacy printer (`handle_orders_list_display` in `parser/dms.rs`) stays for other callers.

**Why**: the legacy printer prints directly; a pure renderer is required. Parity is checked field by field, as in the pilot.

## D11. Legacy removal

**Decision**: remove `Commands::OrdersInfo`, `cli/orders_info.rs` and its `legacy.rs` arm; the registry entry `order.info`
moves to the path `order info` and the new `Request` capability. No alias. The message-building tests for the `orders`
action in `cli_functions.rs` stay (they test core messages, not the command).

**Consequence**: the MCP `get_order` tool calls `ordersinfo` and stops working against a CLI with this change until the MCP
is migrated; the docs already tell MCP users to stay on the older CLI.

## D12. `cli.rs` size

`cli.rs` has 316 production lines. Adding a third migrated dispatch would approach the limit, so the first
implementation step extracts the migrated dispatch (listing, discovery, info) into `cli/migrated.rs`, behavior-preserving,
as T011 did for the earlier split.
