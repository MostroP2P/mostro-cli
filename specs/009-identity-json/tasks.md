# Tasks: Identity Commands

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as for add-invoice: four slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`. Tests before
code, a real failure before a change. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked` (check for FAILED lines explicitly),
`cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, node, relay or regtest; no Git mutation without
authorization.

## Slices

- [x] **1. Characterize and extract.** (done 2026-11-05; evidence in `tests/TEST_SUMMARY.md`) Failing process tests first: `identity init` and
  `identity show` do not exist, and a store created by the legacy first run (`getlasttradeindex` with an unusable relay list) is the reference.
  Move the first-run creation from `db.rs` to `store/create.rs` unchanged and add `create_store`; extend `identity.rs` with `load_identity_from`.
- [x] **2. Outcomes and schemas.** (done 2026-11-05) `outcome/identity.rs`, `identity_exists`, `Stages::Local`, the index schema, output
  schemas for both names, registry entries, discovery.
- [x] **3. The commands.** (done 2026-11-05) Grammar, `commands/identity.rs`, `cli/identity.rs`, `parse_human/identity.rs`, dispatch; binary
  scenarios: create and show in both modes, the legacy comparison, a second init, missing and damaged stores, an uncreatable directory, a store
  used at once by `order new`, secret sweeps, unwritable stdout, two inits at once (a WAL store without its `-shm` file is a unit test of
  `identity.rs`).
- [x] **4. Fixtures, examples, docs.** (done 2026-11-05) Regenerated `docs/schemas`, `contracts/examples.json` with its tests, command reference,
  README, architecture, `AGENTS.md`, requirement evidence.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001 | 2, 3 |
| FR-002, FR-003, FR-007 | 1, 3 |
| FR-004 | 1, 3 |
| FR-005 | 3 |
| FR-006 | 2, 4 |
| SC-001 to SC-006 | 3, 4 |
