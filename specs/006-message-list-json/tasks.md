# Tasks: Message List

**Design**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as for add invoice: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each slice for review. Tests before
code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked --no-fail-fast` (count the FAILED lines), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or
regtest without authorization.

## Slices

- [x] **1. Characterize and extend the fixtures.** (done 2026-11-01; evidence in `tests/TEST_SUMMARY.md`) Pin what `getdm` does (the key loop and ordering, the window, the saves, the grammar and registration). Build the mailbox fixture
  that publishes node messages to chosen trade keys, with self-tests. No production change beyond testable extractions.
- [x] **2. Keys, records and outcomes.** (done 2026-11-01; evidence in `tests/TEST_SUMMARY.md`) `Store::trade_keys` / the `--order` key, `outcome/messages.rs` (records, payload summary, `next_step`, input), the code `store_unreadable` and the
  warning `trade_keys_limited`, schema tests.
- [x] **3. The read and the operation.** (done 2026-11-01; evidence in `tests/TEST_SUMMARY.md`) `msg::inbox::read` and `commands/message/list.rs` against the mailbox: every message kind, filtering by author, key, window and shape, duplicates,
  the three caps, `--order`, failures, no write.
- [x] **4. The command.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) **4a**: grammar (`message list`), registry (`message.list` as `Read`), `cli/message.rs`, `parse_human/message.rs`, removal of `getdm`, generated schema and fixture,
  README and command-reference migration. **4b**: binary scenarios.
- [ ] **5. Failure matrix and examples.** Identity, configuration, relays, unreadable store, invalid input, secret and invoice sentinels, unwritable streams; `contracts/examples.json`
  and its validation.
- [ ] **6. Docs, boundaries, verification.** Architecture, command reference, README migration (MCP), requirement evidence, quickstart with the probe procedure; the probe stays open.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-002 | 2, 4 |
| FR-003 | 2, 3 |
| FR-004, FR-005 | 3 |
| FR-006 | 2, 3, 4 |
| FR-007 | 2, 4, 5 |
| FR-008, FR-009 | 3, 4, 5 |
| FR-010 | 4 |
| SC-001 to SC-006 | 3, 4, 5, 6 |
