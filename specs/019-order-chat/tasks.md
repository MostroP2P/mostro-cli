# Tasks: Order Chat

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md): tracked only here, one dated evidence entry per unit in `tests/TEST_SUMMARY.md`.
Tests before code. Gates: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked` (check FAILED lines), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. Loopback
relays, isolated stores and the scripted node only; no public node.

- [x] **1. The counterparty lookup.** Failing tests first (`outcome::tests` with `counterparty_unknown`, the `msg::counterparty` unit
  tests, `tests/order_chat_process.rs` `mod store` and `mod counterparty`), then `store/counterparty.rs`, `msg/counterparty.rs` (with
  `counterparty_from_trade_pubkeys` moved from `parser/dms.rs`), `commands/order/counterparty.rs`, the code; this folder.
- [x] **2. Chat transport, outcome types, renderer.** Failing tests first (the `msg::chat`, `outcome::chat`, `parse_human::chat` and
  `parse_human::table` unit tests and `tests/order_chat_process.rs` `mod transport`, run against stubs), then `msg/chat.rs` (`read_chat`,
  `open_lines`, `send_chat`, `Publishing::settle`), `outcome/chat.rs`, `Stages::Chat`, `escape_untrusted`, `parse_human/chat.rs`.
  Measured: `output_schema::<ChatData, ()>(&chat_rules())` pretty-printed is 492 lines (the same method gives `order.sync` 694, its
  fixture's size). Projected unit 4: about 1,015 generated lines (`docs/schemas/order-chat-v1.json` 492; discovery about +523: the output
  schema, an input schema of about 27 lines and the descriptor, minus the 12-line human-only entry of unit 3, plus `order.chat` in
  `output.applies_to`) and about 300 of code, tests and documents, so about 1,320, under 1,450: unit 4's JSON tests keep their planned
  scope.
- [x] **3. The command in human mode.** Failing tests first (the registry mapping, `contracts/examples.json` missing,
  `tests/order_chat_process.rs` `mod operation` and `mod binary` against a stub operation and no command), then `commands/order/chat.rs`,
  `cli/order_chat.rs`, `ChatArgs` and `OrderCommands::Chat`, registry `Unsupported`, the dispatch arm, discovery (+13 lines, the human-only
  entry), `contracts/examples.json`, command reference, README, architecture, AGENTS.md. Dispatch: `cli/migrated.rs` measured 393
  production lines at the unit's start, so the guarded path was taken: `if output == OutputMode::Human` on an arm added to the existing
  match of the commands on an order already held, and `order_chat` in an existing import list; 396 lines after it. `request_options` did
  not move. The book update is its own PR.
- [x] **4. JSON.** Failing tests first (the registry mapping and machine list, the `order.chat` input schema and configuration, the global
  options, the missing fixture, the legacy JSON count, `tests/order_chat_process.rs` `mod json`), then registry `Read`, the unguarded arm,
  the `--yes` rule, the schema arms, `docs/schemas/order-chat-v1.json` (492 lines) and the regenerated discovery (+525, -4), command
  reference, README, architecture, AGENTS.md; spec status implemented. The book update is its own PR.
