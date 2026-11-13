# Tasks: `admin order-info`

**Mode**: lean, as [spec 022](../022-order-book-limit/tasks.md) (same gates). No public node. No Git mutation without
authorization. The pull request stays under 1,500 production lines ([plan.md](plan.md), Pull Request Budget).

- [ ] **1. The `mostro-core` pin.** `Cargo.toml` and `Cargo.lock` at the revision of mostro-core#190; the existing suite still passes.
- [ ] **2. Failing tests.** Unit: the data mapping, the admin key, the reply shapes. Schema: the outcomes and the two new codes.
  Human: the blocks. Process: the scripted node answers `admin-order-info`; Story 1 to Story 5, the malformed reply, the timeout,
  the argument errors, the key never printed.
- [ ] **3. Outcome and codes.** `outcome/admin_order_info.rs`, `AdminKeyMissing` / `AdminKeyInvalid`, the output schema rule.
- [ ] **4. Operation.** `commands/admin/order_info.rs` over `msg::exchange`.
- [ ] **5. Grammar and wiring.** `admin order-info --order`, the registry entry, the discovery descriptor, `cli/admin.rs` with the
  admin key read from the environment, the early arm in `cli/migrated.rs`, the source-boundary lists.
- [ ] **6. Human rendering.** `parse_human/admin_order_info.rs`.
- [ ] **7. Fixtures and docs.** `admin-order-info-v1.json` and `discovery-v1.json` regenerated; `contracts/examples.json`;
  `docs/commands.md`, `docs/architecture.md`, `AGENTS.md`, `tests/TEST_SUMMARY.md`, `tests/README.md`.
- [ ] **8. Gates and budget.** fmt, clippy, test, build, schema drift; production-line count under 1,500.
- [ ] **9. Manual probe (SC-006).** The Ortsom regtest stack with a node built from mostro#1110.

## Before Merging

- [ ] Replace the `mostro-core` git pin by the released version.

## After The Pull Request

- [ ] Follow-ups in the MCP repository, the book and the protocol repository.
