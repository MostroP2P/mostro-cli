# Implementation Plan: Follow An Order Until The Trade Ends

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-09 | **Status**: Units 1 to 3 of 7 implemented (the contract types, the constitution
amendment, the live subscription and its reconnects).

## Stack

Seven stacked units, each a branch on the previous one. The first starts from #245's last unit, `feat/order-chat-json`, and reuses its
lookup, chat transport, `ChatRecord` and chat block. #243 (next steps) stacks on top. Each unit stays within 1500 changed lines,
generated fixtures, documents and tests included.

| Unit | Branch | Content | User-visible | Estimated lines |
| --- | --- | --- | --- | --- |
| 1 | `feat/order-watch-contract` | `outcome/watch.rs` and `outcome/watch/cursor.rs`, `TradeRole`, the constitution amendment, this folder | No | about 960 |
| 2 | `feat/order-watch-subscription` | `msg/watch.rs`, `msg/watch/state.rs`, `Armed::recv`: the live subscription, without reconnects | No | about 1220 |
| 3 | `feat/order-watch-reconnect` | `Session::live`, the dropped subscriptions sent again, the `Backend::Kickable` test relay | No | about 600 |
| 4 | `feat/order-watch-operation` | `commands/order/watch.rs`, `order_key`, `StoredOrder.is_mine`, `parse_human/watch.rs` | No | about 1230 |
| 5 | `feat/order-watch-command` | The command in human mode, registry `Unsupported`, discovery +12, command reference, book | Yes | about 980 |
| 6 | `feat/order-watch-json` | JSON on (`JsonSupport::Read`), discovery regenerated (about +880), `contracts/examples.json`, book | Yes | about 1370 |
| 7 | `feat/order-watch-schema` | `docs/schemas/order-watch-v1.json` and its drift test, the `timeout_empty` example, book | No new behavior | about 990 |

**Fallbacks**, named before the work and measured with `git diff --shortstat` before the documents are written: unit 2 above 1450, the
chat follow (`follow_chat`) and its tests move to unit 3; unit 4 above 1450, the renderer and its tests move to unit 5; unit 6 above 1450,
the example and its test move to unit 7. A unit that still projects above 1450 after its fallback is split again before its documents
are written.

**Dependency (Principle III).** Unit 5 adds `libc` as a direct Unix-only dependency, at the version already in `Cargo.lock` through
tokio: `poll(2)` on stdout is the only way to see a closed pipe while an idle watch writes nothing, and the standard library does not
offer it.

**Configuration.** `MOSTRO_PUBKEY` and `RELAYS` come from the root options or the environment through `cli/write.rs::resolve_reader`, as
for `order chat`. The config file of #244 (spec 018) is developed in a parallel stack and is not below this one; once it lands, the
watch reads it through the same `resolve_reader` and this plan does not restate its rules.

## Data flow (units 4 to 6)

`cli/order_watch.rs::run`:

1. `WatchInput::validate`; a failure is `invalid_arguments` with every stage `not_applicable`, exit 2.
2. `write::resolve_reader(cli, false)`: configuration, the identity and the store, read-only, without the work and transport settings.
3. `commands::order::watch::order_watch(&input, &config, &store, &WatchLimits::default(), now, follow)`, where `follow` is
   `FirstChange` (JSON) or `UntilEnd(&mut sink)` (human):
   - the order's index and trade key (`order_key`, extracted unchanged from `message list`): none is `order_unknown`, no store
     `identity_missing`, an unreadable one `store_unreadable`;
   - the user's side (`order_side`: the order's row, then `TradeRole::of_order`), with the same mapping;
   - the counterparty (`find_counterparty`, D6);
   - `msg::watch::Watch::open` from the start cursor, strict (D4);
   - the loop: `watch.next(deadline)` gives a batch or nothing at the deadline; mail becomes `MessageRecord`, chat `ChatRecord`; with no
     counterparty yet, `counterparty_in` and then `follow_chat`, whose replay is held into the same batch; `ended` from `ends_trade`.
     `FirstChange` returns at the first records, or empty at the deadline. `UntilEnd` delivers the first update even when empty (the
     header), then each non-empty one, and stops at the end, at the deadline or when the sink fails;
   - `watch.close()` within 1 s (an overrun is `cleanup_incomplete`); the warnings merged; the cursor advanced from the observed records.
4. JSON: `parse_machine::emit`. Human: the end or deadline line, warnings on stderr, `error: <fixed message>`; `--verbose` prints
   `verbose: order.watch <ok|code>`.

No database transaction is held across networking. The only write is the lookup's cache write, made before the watch opens.

## Unit 1: the contract (pure)

- `src/outcome/watch.rs`: `COMMAND`, `ERRORS` (12 codes, see the [data model](data-model.md)), `JSON_TIMEOUT_SECS` (30),
  `MAX_TIMEOUT_SECS` (86400), `InvalidWatch`, `WatchInput::validate` (D1, D2), `ends_trade` (D9), `WatchData` and `watch_rules()` (D11).
- `src/outcome/watch/cursor.rs`: `Cursor` (`start`, `parse`, `advance`, `Display`), `CURSOR_MARGIN_SECS` (60), `MAX_CURSOR_IDS` (200),
  `MAX_CURSOR_LEN` and `CURSOR_PATTERN` (D7). It is its own file because `outcome/watch.rs` with it would pass 400 production lines.
- `src/outcome/messages.rs`: `TradeRole { Buyer, Seller }` with `of_order(kind, made)`, which #243 reuses for its next steps.
- `.specify/memory/constitution.md`: amendment 1.0.1 (D13).

## Timing

Default budgets: the lookup contacts no relay when the row holds the key and takes up to 26 s when it reads the node's mail; then 10 s to
connect, 15 s for the opening replay, 15 s more when the counterparty is learned during the call, the deadline, and 1 s of cleanup. A JSON
call takes at most `--timeout` + 67 s, typically `--timeout` + 1 to 3 s; the book tells wrappers to allow `--timeout` + 70 s (100 s at the
default) with a client of that timeout. Human mode without `--timeout` is unbounded (D13).

## Risks

- **D9 depends on the local row.** Rows written by 0.16.3 and earlier record every order as made (`is_mine` true), so for an order taken
  with those versions the side can be wrong and `admin-settled` may end a buyer's watch early. A NULL `is_mine` never ends on
  `admin-settled`. Documented in units 5 and 6.
- **#243 (spec 021) stacks after this.** It must reuse `TradeRole::of_order` and `StoredOrder.is_mine` instead of adding them. Its plan to
  stop a watch after the `rate` that follows an end would change D9 and the "rate without waiting" advice, so it must be rechecked against
  this design.
- **Live relays never fail the watch.** A relay that refuses or drops a subscription while it stays connected is not asked again until it
  reconnects; if every relay drops it, a human watch waits silently and a JSON call returns empty at the deadline. The docs say an empty
  result means nothing was observed, not that nothing happened.
- **Duplicate subscriptions after an SDK resend** (for example an auth-required subscription kept for after AUTH) can race the watch's own
  resend; records stay exact because duplicates are dropped by id.
- **SDK lag.** Live notifications beyond the channel are dropped silently; during live replays reaching the capacity sets `truncated`;
  between replays a counterparty that floods `K_sign` could push a node notification out of the channel, the same limit the reads
  document.
- **Watches that never end**: an untaken order that expires (the node sends the maker nothing), an end before the window or the cursor,
  the follow-ups after an end (`bond-slashed`, `add-bond-invoice`). Documented with `--timeout` and `order info`; JSON loops also stop on
  `order info`'s status.
- **The 10,000-record cap** is not pruned: the opening subscriptions keep since S for the whole run, so an id is never provably outside
  every subscription. A human watch that has shown 10,000 records ends with `read_limit_exceeded`; rerun with `--since` or `--after`.
- **Lookup before the lock.** While the row has no counterparty key, every call runs a since-0 read of the node's mail first, up to 26 s.
- **Interrupt (D10).** Ctrl-C or a kill sends no CLOSE to the relays and gives no JSON document; a JSON call is safe to repeat.
- **`cli/migrated.rs` is at 396 production lines.** Unit 5 re-measures it first; if its arm would pass 400, `order info`'s dispatch moves
  unchanged into a new `cli/info.rs`.
- **Verification scope.** Loopback relays and scripted node mail and chat only; no regtest probe (the local Mortsom stack is in use by
  another session) and no public node.
