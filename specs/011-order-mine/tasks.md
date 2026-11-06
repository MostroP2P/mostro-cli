# Tasks: order mine

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean: tracked only here, one evidence entry in `tests/TEST_SUMMARY.md` (shared with specs 012-015 of the same batch). Tests before
code. Gates: fmt, clippy `-D warnings`, `cargo test --locked` (check FAILED lines), build, `cargo +1.94.0 check --locked --all-targets
--all-features`. No push, node, relay or regtest; no Git mutation without authorization.

- [x] **1. Failing tests.** Process tests in `tests/order_mine_process.rs` against the binary (the command did not exist).
- [x] **2. Store, outcome, operation, command.** `store/rows.rs`, `outcome/mine.rs`, `commands/order/mine.rs`, `cli/mine.rs`,
  `parse_human/mine.rs`, grammar, registry, discovery.
- [x] **3. Fixtures, examples, docs.** `docs/schemas/order-mine-v1.json`, `contracts/examples.json` with its tests, command reference,
  README, architecture, requirement evidence.
