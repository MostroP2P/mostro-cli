# Tasks: Trade Steps

**Design**: [spec](spec.md), [plan](plan.md).

**Mode**: lean, as for add invoice: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each
slice for review. Tests before code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --no-fail-fast` (check for FAILED lines),
`cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or regtest without authorization.

## Slices

- [x] **1. Characterize and extend the fixtures.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) Pin what `fiatsent`/`release`/`cancel` do today (grammar, payload bytes, the range `next_trade`
  condition and index update). Extend the scripted daemon with the replies and refusals of the source, including both cooperative-cancel steps.
  No production change.
- [x] **2. Outcomes.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) `outcome/step.rs`, schemas for the three names, `Store` helpers if missing.
- [x] **3. The operation.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) Every accepted reply, refusals, silence, unusable reply, decoy replies, forced status-update failure, the
  `range_not_continued` warning (new warning code), work stopping before anything is sent.
- [x] **4. The commands.** (4a and 4b done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) **4a**: grammar, registry, `cli/step.rs`, `parse_human/step.rs`, removal of the legacy arms, schemas and fixtures,
  README and command-reference migration. **4b**: binary scenarios.
- [x] **5. Failure matrix and examples.** (done 2026-11-02; evidence in `tests/TEST_SUMMARY.md`) Identity, configuration, protocol, work, relay, unwritable store, invalid input, secret sentinels,
  unwritable streams; `contracts/examples.json` and its validation.
- [x] **6. Docs, boundaries, verification.** (done 2026-11-02; the regtest probe stays open) Architecture, command reference, README migration, requirement evidence, quickstart with the
  regtest probe procedure; the probe stays open until separately authorized.

**Follow-up (required, not part of this spec)**: spec 008, range children (next trade key, reserved index, child order row).

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-002, FR-003 | 2, 4 |
| FR-004, FR-005 | 3, 4 |
| FR-006, FR-007 | 2, 3, 4 |
| FR-008 | 2, 4, 5 |
| FR-009 | 4, 5 |
| FR-010 | 4 |
| SC-001 to SC-006 | 4, 5, 6 |
