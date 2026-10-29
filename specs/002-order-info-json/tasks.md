# Tasks: Order Info

**Design**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md),
[CLI contract](contracts/cli.md), [validation guide](quickstart.md).

**Mode**: lean, agreed 2026-10-28. Six vertical slices instead of 44 numbered tasks. The rules that
matter stay: tests before code (with `#[ignore = "red until <slice>"]` where a test lands earlier than
its code), a real failure shown before a repair, mutation spot checks for new guards, and the
required gates on every slice. What is dropped: per-task evidence sections and status pointers in
several files. Each slice is one or two commits and one short entry in `tests/TEST_SUMMARY.md`.
This file is the only place that tracks progress.

**Gates for every slice**: `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked`,
`cargo build --locked`, and `cargo +1.94.0 check --locked --all-targets --all-features`.
No push, public-node use or regtest without authorization. Native Windows is a follow-up on a
native machine.

## Slices

- [x] **1. Characterize and build the test fixtures.** (done 2026-10-28; evidence in `tests/TEST_SUMMARY.md`)
  Pin what `ordersinfo` does today (grammar, message, reply handling) and the wire facts of the
  `orders` request and the `CantDo` replies with the pinned `mostro-core`. Move the process harness
  out of `tests/order_list_process.rs` into `tests/common/harness.rs` (the 46 existing process tests
  unchanged). Build the scripted fake daemon and identity-store seeding in `tests/common/fake_mostro.rs`
  with self-tests. No production change.
- [x] **2. Identity and outcome pieces.** (done 2026-10-29; evidence in `tests/TEST_SUMMARY.md`)
  `src/identity.rs` (read-only loader; `db.rs` delegates its derivation), the new codes, warnings, stage
  constructors, optional `error.reason` and command-scoped schema sets in `src/outcome.rs` and
  `src/outcome/schema.rs`, `JsonSupport::Request`, and the typed data in `src/outcome/info.rs`. The
  existing `docs/schemas` fixtures must not change.
- [ ] **3. The exchange.**
  `src/msg/node_info.rs`, `send_msg.rs`, `reply.rs`, `exchange.rs` and `ExchangeLimits` behind the
  private session: one request, correlated reply, bounded cleanup, no retry. Contract tests against
  the fake daemon; privacy doctests stay green.
- [ ] **4. The command, machine and human.**
  Extract `src/cli/migrated.rs` from `src/cli.rs`; `OrderCommands::Info` and the registry move; remove
  `ordersinfo`, `src/cli/orders_info.rs` and the legacy arm; `src/commands/order/info.rs`; the dispatch;
  `src/parse_human/order_info.rs`. Binary scenarios: success, subset, repeated id, `not_found`, noise
  around the reply, refusal, timeout, one request, byte-identical identity store, parity of both modes.
- [ ] **5. Failures, schemas, fixtures.**
  The full failure matrix (identity, configuration, protocol, work, rejected request, relay, malformed
  reply, many ids, secret sentinels, unwritable streams); `order.info` schemas; regenerate
  `docs/schemas/discovery-v1.json`, add `docs/schemas/order-info-v1.json` and
  `contracts/examples.json`; extend the drift tests. `order-list-v1.json` stays unchanged.
- [ ] **6. Docs, boundaries, verification.**
  Update the frozen counts in the boundary tests and the CI wiring test deliberately; architecture,
  command reference, README and the `ordersinfo` migration (the MCP `get_order` tool stops working
  until it is migrated); Linux quickstart run and final gates. The regtest probe needs separate
  authorization and stays open until run. Then the requirement evidence in `tests/README.md` and the
  status lines of the spec, plan, `AGENTS.md` and the roadmap, stopping without committing that last edit.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-001a (ids, no cap, not returned, `not_found`) | 2, 4 |
| FR-002 (read-only, one request, identity key) | 2, 3, 4 |
| FR-003 (correlation, deadline, cleanup) | 3 |
| FR-004, FR-006 (outcome, parity, default human) | 2, 4 |
| FR-005, FR-007 (stages, stable failures) | 2, 5 |
| FR-008 (discovery, fixtures, unmigrated guard) | 5 |
| FR-009 (shared transport boundary) | 3, 6 |
| FR-010 (untrusted text, diagnostics) | 4, 5 |
| FR-011 (documented contract) | 6 |
| SC-001 to SC-006 | 4, 5, 6 |
