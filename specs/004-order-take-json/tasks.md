# Tasks: Order Take

**Design**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as for order info and order new: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop
after each slice for review. Tests before code (`#[ignore = "red until <slice>"]` when a test lands early), a real failure before a repair,
mutation spot checks for new guards. Gates per slice: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D
warnings`, `cargo test --locked`, `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or
regtest without authorization.

## Slices

- [ ] **1. Characterize and extend the fixtures.** Pin what `takesell`/`takebuy` do today: grammar, the payload bytes for every combination of
  invoice and amount, `taken_sell_order`, the local effects of `save_order`. Extend the scripted daemon with the take replies of the source
  (`add-invoice` with the order, `waiting-seller-to-pay` with no payload and an echoed message id, `pay-invoice` with the hold invoice, `pay-bond-invoice`) and
  the refusals it models (`pending_order_exists`, `invalid_order_status`, own order, `out_of_range_sats_amount`, `invalid_invoice`, `invalid_trade_index`), with
  a stateful order book. No production change.
- [ ] **2. Shared dispatch and outcomes.** Extract `cli/write.rs` from `cli/order_new.rs` (order new tests unchanged); `outcome/take.rs` (input,
  payload, `NextStep`, data, minimal order) with unit and schema tests for the two canonical names.
- [ ] **3. The operation.** `commands/order/take.rs` for both sides against the scripted daemon: each accepted reply and its saved row, refusals, silence,
  unusable reply, bond, forced save failure (two shapes), work stopping before anything is sent, index policy.
- [ ] **4. The commands.** Split in two commits. **4a**: grammar, registry (`order.take-sell`, `order.take-buy`), `cli/order_take.rs`,
  `parse_human/order_take.rs`, removal of `takesell`, `takebuy`, `cli/take_order.rs` and the book read, generated schemas and fixtures, README and
  command-reference migration. **4b**: binary scenarios (success per side and invoice case, `--yes` rules, prompt, refusal, silence, bond, unsaved take, range
  amount, verbose, hold invoice only on stdout).
- [ ] **5. Failure matrix and examples.** Identity, configuration, protocol, work, rejected and unreachable relay, unwritable store, invalid input, secret
  sentinels (including the invoice and the hold invoice), unwritable streams; `contracts/examples.json` and its validation.
- [ ] **6. Docs, boundaries, verification.** Architecture, command reference, README migration (MCP `take_order`), requirement evidence, quickstart with the
  regtest probe procedure, status lines; the probe stays open until separately authorized.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-002, FR-003 | 2, 4 |
| FR-004, FR-005 | 3, 4 |
| FR-006, FR-007 | 2, 3, 4 |
| FR-008, FR-009 | 3, 4, 5 |
| FR-010, FR-011 | 2, 4, 5 |
| FR-012 | 4, 5 |
| FR-013 | 4 |
| SC-001 to SC-006 | 4, 5, 6 |
