# Tasks: Trade Steps

**Design**: [spec](spec.md), [plan](plan.md).

**Mode**: lean, as for add invoice: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`, stop after each
slice for review. Tests before code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --no-fail-fast` (check for FAILED lines),
`cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, public-node use or regtest without authorization.

## Slices

- [ ] **1. Characterize and extend the fixtures.** Pin what `fiatsent`/`release`/`cancel` do today (grammar, payload bytes, the range `next_trade`
  condition and index update). Extend the scripted daemon with the replies and refusals of the source, including both cooperative-cancel steps.
  No production change.
- [ ] **2. Outcomes.** `outcome/step.rs`, schemas for the three names, `Store` helpers if missing.
- [ ] **3. The operation.** Every accepted reply, refusals, silence, unusable reply, decoy replies, forced status-update failure, range index
  policy, work stopping before anything is sent.
- [ ] **4. The commands.** **4a**: grammar, registry, `cli/step.rs`, `parse_human/step.rs`, removal of the legacy arms, schemas and fixtures,
  README and command-reference migration. **4b**: binary scenarios.
- [ ] **5. Failure matrix and examples.** Identity, configuration, protocol, work, relay, unwritable store, invalid input, secret sentinels,
  unwritable streams; `contracts/examples.json` and its validation.
- [ ] **6. Docs, boundaries, verification.** Architecture, command reference, README migration, requirement evidence, quickstart with the
  regtest probe procedure; the probe stays open until separately authorized.

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
