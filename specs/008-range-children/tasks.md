# Tasks: Range Order Children

**Design**: [spec](spec.md), [plan](plan.md).

**Mode**: lean, as for trade steps: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each
slice for review. Tests before code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --no-fail-fast` (check for FAILED lines),
`cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or regtest without authorization.

## Slices

- [x] **1. The maker marker.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) Failing tests for takers and makers; `save_order` records `is_mine` exactly; `range_has_room` requires it; the
  spec 007 range tests and documents are amended; the false positive is pinned and gone.
- [x] **2. The scripted node creates children.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) Extend the step daemon: a range order, the key stored by a buy order's `fiat-sent`, the child
  created at `release` and its `new-order` sent to the new key with the echoed request id. No production change.
- [x] **3. Key and index in the steps.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) The conditions of the spec, the index policy (reserved before, spent after), `next_trade_index`,
  `next_step` `sync_child_order`, the range warning only for inexact rows; operation and binary tests.
- [ ] **4. `order sync`.** Outcomes, schema, the operation against the scripted mailbox, the command, human output, fixtures, discovery.
- [ ] **5. Failure matrix and examples.** Identity, configuration, store, relay, unwritable streams, sentinels; `contracts/examples.json`.
- [ ] **6. Docs, boundaries, verification.** Architecture, command reference, README, requirement evidence, quickstart with the regtest probe
  procedure for a range order; the probe stays open until separately authorized.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001 | 1 |
| FR-002, FR-003 | 3 |
| FR-004, FR-005, FR-006 | 4, 5 |
| FR-007 | 1, 6 |
| SC-001, SC-002 | 2, 3, 4 |
| SC-003 to SC-005 | 4, 5, 6 |
