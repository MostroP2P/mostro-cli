# Tasks: Order New

**Design**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as for order info: six slices tracked only here, one short evidence entry per slice in `tests/TEST_SUMMARY.md`,
stop after each slice for review. Tests before code (`#[ignore = "red until <slice>"]` when a test lands early), a real failure before a
repair, mutation spot checks for new guards. Gates per slice: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets
--all-features -- -D warnings`, `cargo test --locked`, `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`.
No push, public-node use or regtest without authorization.

## Slices

- [x] **1. Characterize and extend the fixtures.** (done 2026-10-29; evidence in `tests/TEST_SUMMARY.md`) Pin what `neworder` does today (grammar, message built, reply handling, what is saved,
  when the index is written). Extend the scripted daemon: `new-order` replies addressed to the trade key, `CantDo` reasons, the bond reply, a
  daemon-side trade-index check; a seeded writable identity store with a user row. No production change.
- [ ] **2. Exchange and storage.** `Outbound` with identity and trade keys, wait for the trade key, `PayBondInvoice` returned to the caller;
  `src/store.rs` (reserve, commit order and index in one transaction, advance, typed errors); keep the order-info tests green.
- [ ] **3. Outcomes.** `uncertain` constructors and per-command schema rules, new codes, `outcome/new.rs` (`OrderNewInput`, `OrderNewData`),
  `JsonSupport::Request` for `order.new` with the generated schema and fixtures (`order-new-v1.json`, regenerated discovery).
- [ ] **4. The command.** `OrderCommands::New`, the registry move, `commands/order/new.rs`, the confirmation in `cli/migrated.rs`, the human
  preview and result, removal of `neworder`, `cli/new_order.rs` and the Yadio call. Binary scenarios: accepted and saved, refusal, silence,
  bond, forced save failure, `--yes` rules, one request, index and database effects.
- [ ] **5. Failure matrix and examples.** Identity, configuration, protocol, work, rejected relay, ambiguous publication, many bad inputs,
  secret sentinels, unwritable streams, stdin never read; `contracts/examples.json` and its validation.
- [ ] **6. Docs, boundaries, verification.** Boundary counts, architecture, command reference, README migration (MCP `create_order`),
  Linux quickstart run, requirement evidence, status lines; the regtest probe stays open until separately authorized.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-002, FR-003 | 3, 4 |
| FR-004, FR-005 | 2, 4 |
| FR-006, FR-007, FR-008 | 2, 3, 4, 5 |
| FR-009, FR-010 | 3, 5 |
| FR-011 | 2, 6 |
| FR-012 | 4 |
| SC-001 to SC-006 | 4, 5, 6 |
