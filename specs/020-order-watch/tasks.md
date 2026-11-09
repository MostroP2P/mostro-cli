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
- [ ] **3. Reconnects.** Failing tests first (the reconnect tests fail on unit 2), then `Session::live`, the dropped subscriptions sent
  again on Connected, the reconnect as a replay start, `Backend::Kickable` in the test harness. Moved from unit 2: `follow_chat(peer)`
  (the chat REQs on every relay, waiting for their replays), which skips a relay that went away (a test drops one first and expects no
  wait for the read budget) and records which filter each relay lacks, so D5 can send it again.
- [ ] **4. The operation and the human renderer.** Failing tests first (`tests/order_watch_process.rs` `mod operation`, the
  `parse_human::watch` unit tests), then `commands/order/watch.rs`, `order_key`, `StoredOrder.is_mine`, `parse_human/watch.rs`.
  Fallback above 1450: the renderer and its tests move to unit 5.
- [ ] **5. The command in human mode.** Failing tests first (the registry mapping, the legacy JSON list, the discovery fixture,
  `tests/order_watch_process.rs` `mod binary`), then `WatchArgs`, `OrderCommands::Watch`, `cli/order_watch.rs`, registry `Unsupported`,
  the dispatch arm (re-measure `cli/migrated.rs` first), discovery, command reference, README, architecture, AGENTS.md. The book update is
  its own PR.
- [ ] **6. JSON.** Failing tests first, then registry `Read`, the unguarded arm, the schema arms, the regenerated discovery,
  `contracts/examples.json` (`new_records`). Fallback above 1450: the example and its test move to unit 7. The book update is its own PR.
- [ ] **7. The output schema file.** Failing first (the fixture is missing), then `docs/schemas/order-watch-v1.json`, its drift test, the
  `timeout_empty` example; spec status implemented. The book update is its own PR.
