# Tasks: Order Watch

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as [spec 019](../019-order-chat/tasks.md): tracked only here, one dated evidence entry per unit in
`tests/TEST_SUMMARY.md`. Tests before code. Gates: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked` (check FAILED lines), `cargo build --locked`,
`cargo +1.94.0 check --locked --all-targets --all-features`. Loopback relays, isolated stores and scripted node and chat events only; no
public node.

- [x] **1. The contract.** Failing tests first (`outcome::watch` and `outcome::watch::cursor` unit tests and
  `trade_role_of_order_maps_all_four_pairs`, which did not compile), then `outcome/watch.rs`, `outcome/watch/cursor.rs` (split off so
  that neither passes 400 production lines), `TradeRole`, the constitution amendment 1.0.1 and this folder. Measured:
  `output_schema::<WatchData, ()>(&watch_rules())` pretty-printed is 842 lines (the same method gives 492 for `order.chat` and 851 for
  `message.list`, their fixtures' sizes). Projected: unit 6 about 890 generated lines in `discovery-v1.json` (the output schema, an input
  schema of about 30 lines and the descriptor, minus the 12-line human-only entry of unit 5) and about 450 of code, tests and documents,
  so about 1,340, under 1,450; unit 7 about 842 generated lines and about 150 more, so about 990. The fallbacks of the
  [plan](plan.md#stack) stand.
- [x] **2. The live subscription.** Failing tests first (`msg::watch::state` unit tests, `tests/order_watch_process.rs` `mod live`), then
  `msg/watch.rs`, `msg/watch/state.rs`, `Armed::recv` and `inbox::mail_filter` (shared with `read_inbox`); a live CLOSED or disconnect
  drops that subscription or relay until unit 3. The review fixes (every stored event counts toward `truncated`, one held entry per
  record, the chat window on the envelope's time) reached about 1,610 lines, so the fallback applied: `follow_chat`, its tests and the
  live-disconnect process test moved to unit 3. Production lines 338 (`msg/watch.rs`) and 379 (`msg/watch/state.rs`).
- [x] **3. Reconnects.** Failing tests first (the reconnect tests fail on unit 2), then `Session::live`, the dropped subscriptions sent
  again on Connected, the reconnect as a replay start, `Backend::Kickable` in the test harness. Moved from unit 2: `follow_chat(peer)`
  (the chat REQs on every relay, waiting for their replays), which skips a relay that went away (a test drops one first and expects no
  wait for the read budget) and records which filter each relay lacks, so D5 can send it again. Found: the SDK resends a subscription
  only when the reconnect falls in a later whole second than its REQ, so a 200 ms test retry never resent; tests use 1 s and
  `WatchLimits::retry` documents the floor. `Seen` and `Batch` moved to `msg/watch/records.rs` and `WatchLimits` and `WatchFailure` to
  `msg/watch/limits.rs`, unchanged, to keep every file under 400 production lines (378 `watch.rs`, 372 `state.rs`). Review fixes: a
  live replay cut before EOSE gets the EOSE `truncated` verdict, and a CLOSED id is unsubscribed so the SDK does not send a
  `rate-limited:` one again beside the new id (389 `watch.rs`, 381 `state.rs`).
- [x] **4. The operation and the human renderer.** Failing tests first (`tests/order_watch_process.rs` `mod operation`, which did not
  compile, and the `parse_human::watch` unit tests, which failed against stubs), then `commands/order/watch.rs`, `order_key` (extracted
  unchanged from `message list`), `StoredOrder.is_mine`, `parse_human/watch.rs` (the message and chat blocks made `pub(crate)`).
  `order_watch` returns `Watched { outcome, ended_by }`: the human end line names the ending action, which only the operation can tell
  (it knows the side). The renderer takes `Shown::First { since }` or `Shown::Later { learned }` in place of a `header` flag, because the
  header shows the window start and a later update announces a learned counterparty. With no `counterparty_not_saved` warning (D11), the
  frozen-store test checks that a failed cache write leaves the outcome warning-free and conforming. Measured before documents: 1,092
  changed lines, so the fallback did not apply. Production lines 264 (`commands/order/watch.rs`) and 96 (`parse_human/watch.rs`).
- [ ] **5. The command in human mode.** Failing tests first (the registry mapping, the legacy JSON list, the discovery fixture,
  `tests/order_watch_process.rs` `mod binary`), then `WatchArgs`, `OrderCommands::Watch`, `cli/order_watch.rs`, registry `Unsupported`,
  the dispatch arm (re-measure `cli/migrated.rs` first), discovery, command reference, README, architecture, AGENTS.md. The book update is
  its own PR.
- [ ] **6. JSON.** Failing tests first, then registry `Read`, the unguarded arm, the schema arms, the regenerated discovery,
  `contracts/examples.json` (`new_records`). Fallback above 1450: the example and its test move to unit 7. The book update is its own PR.
- [ ] **7. The output schema file.** Failing first (the fixture is missing), then `docs/schemas/order-watch-v1.json`, its drift test, the
  `timeout_empty` example; spec status implemented. The book update is its own PR.
