# Tasks: `admin order-info`

**Mode**: lean, as [spec 022](../022-order-book-limit/tasks.md) (same gates). No public node. No Git mutation without
authorization. The pull request stays under 1,500 production lines ([plan.md](plan.md), Pull Request Budget).

- [x] **1. The `mostro-core` pin.** `Cargo.toml` and `Cargo.lock` at the revision of mostro-core#190; the existing suite still passes.
- [x] **2. Tests.** Unit: the data mapping, the admin key, the reply shapes. Schema: the outcomes and the two new codes.
  Human: the blocks. Process: the scripted node answers `admin-order-info`; Story 1 to Story 5, the malformed reply, the timeout,
  the argument errors, the key never printed. They were written after the code of tasks 3 to 6, not before it. In place of a
  failing run, three mutations were applied and reverted, and each failed its tests in the unit and in the process suite: the
  order-id check removed from `answer`, `status` left unescaped in the human view, and a missing key reported as invalid.
- [x] **3. Outcome and codes.** `outcome/admin_order_info.rs`, `AdminKeyMissing` / `AdminKeyInvalid`, the output schema rule.
- [x] **4. Operation.** `commands/admin/order_info.rs` over `msg::exchange`.
- [x] **5. Grammar and wiring.** `admin order-info --order`, the registry entry, the discovery descriptor, `cli/admin.rs` with the
  admin key read from the environment, the early arm in `cli/migrated.rs`, the source-boundary lists.
- [x] **6. Human rendering.** `parse_human/admin_order_info.rs`.
- [x] **7. Fixtures and docs.** `admin-order-info-v1.json` and `discovery-v1.json` regenerated; `contracts/examples.json`;
  `docs/commands.md`, `docs/architecture.md`, `AGENTS.md`, `tests/TEST_SUMMARY.md`, `tests/README.md`.
- [x] **8. Gates and budget.** fmt, clippy, test, build, schema drift; production-line count under 1,500.
- [x] **9. Manual probe (SC-006).** Run by hand on 2026-11-13 against a regtest `mostrod` built from mostro#1110 (not the Ortsom
  stack), with a registered solver's key in `ADMIN_NSEC`. One buy order with maker and taker bonds was asked about twelve times
  along its life, and the node's log shows every query answered: `waiting-maker-bond`, `pending`, `waiting-taker-bond`,
  `waiting-payment` (hold invoice `open`), `waiting-buyer-invoice`, `active` and `fiat-sent` (`accepted`), `settled-hold-invoice`
  four times (`settled`, `failed_payment` with the attempts counted) and `success`; an earlier order was seen `pending` and then
  `canceled`. Each snapshot agrees with the messages the node sent the parties just before it. Not covered: the hold invoice
  state was not cross-checked with `lncli`, and the `unavailable` and `invalid_pubkey` cases were not run against the real node.

## Before Merging

- [ ] Replace the `mostro-core` git pin by the released version.

## After The Pull Request

- [ ] Follow-ups in the MCP repository, the book and the protocol repository.
