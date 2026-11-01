# Tasks: Add Invoice

**Design**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as for order take: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each
slice for review. Tests before code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked` (check for FAILED lines explicitly), `cargo build --locked`,
`cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or regtest without authorization.

## Slices

- [x] **1. Characterize and extend the fixtures.** (done 2026-11-01; evidence in `tests/TEST_SUMMARY.md`) Pin what `addinvoice`/`addbondinvoice` do today (grammar, payload bytes, the bolt11-only check, the
  timeout-is-success reading). Extend the scripted daemon with the add-invoice replies and refusals and the bond payout mode (ack with no request id,
  and silent). No production change.
- [ ] **2. Store, correlation and outcomes.** `Store::trade_for_order`/`set_status`, `by_order` correlation in `msg::exchange`, `outcome/invoice.rs`,
  `order_unknown`, schemas for both names.
- [ ] **3. The operations.** Both commands against the scripted daemon: every accepted reply, refusals, silence, unusable reply, decoy replies,
  forced status-update failure, work stopping before anything is sent, no index spent.
- [ ] **4. The commands.** **4a**: grammar (`bond` group), registry, `cli/invoice.rs`, `parse_human/invoice.rs`, removal of the legacy commands, generated
  schemas and fixtures, README and command-reference migration. **4b**: binary scenarios.
- [ ] **5. Failure matrix and examples.** Identity, configuration, protocol, work, rejected and unreachable relay, unwritable store, invalid input,
  secret sentinels (including the invoice), unwritable streams; `contracts/examples.json` and its validation.
- [ ] **6. Docs, boundaries, verification.** Architecture, command reference, README migration (MCP `add_invoice`), requirement evidence, quickstart with
  the regtest probe procedure for `order add-invoice`; the probe stays open until separately authorized.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-002, FR-003 | 2, 4 |
| FR-004, FR-005 | 2, 3, 4 |
| FR-006, FR-007 | 2, 3, 4 |
| FR-008, FR-009 | 2, 4, 5 |
| FR-010 | 4 |
| SC-001 to SC-006 | 3, 4, 5, 6 |
