# Tasks: Invoice From Standard Input

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean: three slices tracked only here, one short evidence entry in `tests/TEST_SUMMARY.md`. Tests before code, a real failure before a
change. Gates: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked` (check
for FAILED lines explicitly), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, node, relay or
regtest; no Git mutation without authorization.

## Slices

- [x] **1. Failing tests.** (done 2026-11-05; evidence in `tests/TEST_SUMMARY.md`) Process tests for the four commands against the scripted node
  (the stdin invoice published, each invalid input refused with nothing published, `--yes` required with stdin left open, sentinels) and the
  discovery marker test; they fail because `-` is taken as the invoice itself.
- [x] **2. The reader and the dispatch.** (done 2026-11-05) `cli/stdin.rs` with unit tests, the four dispatchers, the `--invoice` help.
- [x] **3. Discovery and docs.** (done 2026-11-05) `x-stdin` in the input schemas, regenerated discovery, command reference, README, the stdin
  sentence of the contracts of specs 003-005 marked "Amended 2026-11-05 by spec 010", requirement evidence.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001 to FR-005 | 1, 2 |
| FR-006 | 1, 3 |
| SC-001 to SC-005 | 1, 2, 3 |
