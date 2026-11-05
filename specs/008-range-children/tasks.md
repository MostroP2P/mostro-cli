# Tasks: Range Order Children

**Design**: [spec](spec.md), [plan](plan.md).

**Mode**: lean, as for trade steps: six slices (and a seventh, added 2026-11-05) tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each
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
- [x] **4. `order sync`.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`; the examples file is slice 5) Outcomes, schema, the operation against the scripted mailbox, the command, human output, fixtures, discovery.
- [x] **5. Failure matrix and examples.** (done 2026-11-03; evidence in `tests/TEST_SUMMARY.md`) Identity, configuration, store, relay, unwritable streams, sentinels; `contracts/examples.json`.
- [x] **6. Docs, boundaries, verification.** (done 2026-11-03; the regtest probe stays open) Architecture, command reference, README, requirement evidence, quickstart with the regtest probe
  procedure for a range order; the probe stays open until separately authorized.
- [x] **7. Reserve the trade index before publishing.** (done 2026-11-05; evidence in `tests/TEST_SUMMARY.md`; follow-up to a confirmed P1 review
  finding on PR #232) The index was read before publishing and recorded only after the exchange, so a run killed in that window, or two runs at
  once, could reuse a trade key (silently for a range continuation, which the node never records). `Store::reserve_trade` now records the next
  index in one `BEGIN IMMEDIATE` transaction committed before anything is sent (`store/reserve.rs`); `order new`, `order take-*` and the range
  continuation reserve right before publishing and give the index back (compare-and-swap to the exact previous value) only after a refusal or a
  request that never left. The CLI peeks only for the human preview. `trade_index_not_recorded` is removed; the legacy `getlasttradeindex` and
  order save only raise the index. Failing process tests first (a continuation and an `order new` killed after publishing, two continuations and
  two `order new` runs at once), then store tests (32 concurrent reservations, a blocked commit, the give-back, a missing or frozen user row).
  Specs 003, 004, 007 and 008 amended ("amended 2026-11-05 by spec 008"); the external JSON interface book is a follow-up.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001 | 1 |
| FR-002, FR-003 | 3, 7 |
| FR-004, FR-005, FR-006 | 4, 5 |
| FR-007 | 1, 6 |
| SC-001, SC-002 | 2, 3, 4 |
| SC-003 to SC-005 | 4, 5, 6 |
| SC-006 | 7 |
