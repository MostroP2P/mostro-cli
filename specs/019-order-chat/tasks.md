# Tasks: Order Chat

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md): tracked only here, one dated evidence entry per unit in `tests/TEST_SUMMARY.md`.
Tests before code. Gates: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked` (check FAILED lines), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. Loopback
relays, isolated stores and the scripted node only; no public node.

- [x] **1. The counterparty lookup.** Failing tests first (`outcome::tests` with `counterparty_unknown`, the `msg::counterparty` unit
  tests, `tests/order_chat_process.rs` `mod store` and `mod counterparty`), then `store/counterparty.rs`, `msg/counterparty.rs` (with
  `counterparty_from_trade_pubkeys` moved from `parser/dms.rs`), `commands/order/counterparty.rs`, the code; this folder.
- [ ] **2. Chat transport, outcome types, renderer.** `msg/chat.rs` (`read_chat`, `open_lines`, `send_chat`, `Publishing::settle`),
  `outcome/chat.rs`, `Stages::Chat`, `escape_untrusted`, `parse_human/chat.rs`. Measure and record here the pretty-printed line count of
  `output_schema::<ChatData, ()>(&chat_rules())` and the projected unit-4 total; above 1450, unit 4's JSON tests shrink to the `--yes`
  refusal and one conformance run.
- [ ] **3. The command in human mode.** `commands/order/chat.rs`, `cli/order_chat.rs`, grammar, registry `Unsupported`, the dispatch arm
  (record here which `cli/migrated.rs` path was taken and its size), discovery, `contracts/examples.json`, command reference, README,
  architecture, AGENTS.md, book.
- [ ] **4. JSON.** Registry `Read`, the `--yes` rule, schema arms, `docs/schemas/order-chat-v1.json`, regenerated discovery, JSON tests,
  command reference, book; spec status implemented.
