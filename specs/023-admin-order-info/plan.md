# Implementation Plan: `admin order-info`

**Spec**: [spec.md](spec.md) | **Contract**: [contracts/cli.md](contracts/cli.md) | **Date**: 2026-11-12 | **Status**: Units 1 to 3 implemented with Linux evidence against the scripted node.

## Pull Request Budget

One pull request, under **1,500 lines of production code** (added plus removed lines in `src/`, excluding `#[cfg(test)]` blocks and
test-only files), measured as in [spec 022](../022-order-book-limit/plan.md#pull-request-budget). Estimate: about 700 lines.

| Unit | Production estimate |
| --- | --- |
| Unit 1: the `mostro-core` pin | 0 (manifest only) |
| Unit 2: outcome, operation, grammar and wiring | ~550 lines |
| Unit 3: human rendering | ~150 lines |

The pull request stays a draft until `mostro-core` is released: the pin by git revision is replaced by the released version first.

## Unit 1: the `mostro-core` pin

- `Cargo.toml`: `mostro-core` from `https://github.com/MostroP2P/mostro-core` at the revision of pull request #190, in place of
  `=0.17.3`; `Cargo.lock` follows. That branch is 0.17.3 plus the new action and payload, and the CLI builds against it unchanged
  (checked with `cargo check --all-targets` before this plan was written).

## Unit 2: outcome, operation, grammar and wiring

New files, each well under 400 production lines:

- `src/outcome/admin_order_info.rs`: `AdminOrderInfoData` and its parts (`HoldInvoiceRecord`, `CashuEscrowRecord`, `PayoutRecord`,
  `BondRecord`), `Serialize` and `JsonSchema`, with the fields of the [contract](contracts/cli.md#data).
  `AdminOrderInfoData::from_reply(node, &OrderInfo) -> (Self, Warnings)` maps every field explicitly. Amounts go through
  `to_string`, times through the `rfc3339_utc` helper `order.info` uses (a time that cannot be represented is `null` with the
  existing `invalid_order_timestamp` warning). The open enums (D7) are the wire names of the `mostro-core` enums, taken with
  `serde_json::to_value`, so the CLI never keeps its own list of names.
- `src/commands/admin/mod.rs` and `src/commands/admin/order_info.rs`: `COMMAND = "admin.order-info"`,
  `admin_order_info(order: Uuid, config, admin: &Keys, options, limits) -> Envelope<AdminOrderInfoData, ()>`. It builds
  `Message::new_order(Some(order), Some(request_id), None, Action::AdminOrderInfo, None)`, calls `msg::exchange` with
  `identity` and `signer` both the admin key, `Lane::FirstContact` and `Expect { allowed_actions: [AdminOrderInfo, CantDo],
  by_order: None }`, and maps the reply (FR-004 to FR-006). No printing, storage, environment or client access, as
  `commands/order/info.rs`.
- `src/cli/admin.rs`: the wrapper `run(cli, command, output) -> i32`. It resolves in order of cost (FR-002): the node lookup and
  `PublicReadConfig::resolve`, `request_options`, then the admin key, then calls the operation and renders. It reuses
  `node_lookup`, `request_options` and `exchange_limits` from `cli/write.rs` and `emit` / `diagnose` as `cli/migrated.rs` does.
- The admin key: a pure `admin_keys(value: Option<&str>) -> Result<Keys, ErrorCode>` (missing or empty is `AdminKeyMissing`, a
  failed `Keys::parse` is `AdminKeyInvalid`) and one call that reads `ADMIN_NSEC` with `env_text(env::var_os(..))`, the way
  `node_lookup` reads the node settings. The error never carries the value.

Changed files:

- `src/cli/commands.rs` (351 lines, near the limit): one `Admin { command: AdminCommands }` variant. `AdminCommands` and its
  `OrderInfo(AdminOrderInfoArgs)` live in `src/cli/command_args.rs` (or a new `src/cli/admin_args.rs` if that file would pass 400
  lines), so `commands.rs` grows by a few lines only.
- `src/cli/migrated.rs`: one early arm, `Some(Commands::Admin { command }) => return Some(admin::run(cli, command, output).await)`,
  placed with the other request commands so the legacy context (which would require a database) is never built.
- `src/cli/registry.rs`: `CommandEntry { path: &["admin", "order-info"], canonical: "admin.order-info", json: JsonSupport::Request }`.
- `src/cli/schema.rs`, `src/cli/input_schema.rs`: the command's descriptor and input schema (one required `order`).
- `src/outcome/codes.rs`: `AdminKeyMissing` and `AdminKeyInvalid` with their names, prose (`"ADMIN_NSEC is not set."`,
  `"ADMIN_NSEC is not a private key."`) and exit 1.
- `src/outcome/schema.rs`, `src/outcome/rules.rs`: the output schema through `OutcomeRules::request`, with this command's code set.
- `src/outcome.rs`, `src/commands/mod.rs`, `src/cli.rs`: module declarations.
- `tests/integration_tests.rs`: the new files join the size, import and no-I/O lists; `ADMIN_NSEC` may be read only in
  `src/cli/admin.rs` (and the legacy `src/cli/context.rs`, unchanged).

Generated, never hand-edited: `docs/schemas/admin-order-info-v1.json` (new) and `docs/schemas/discovery-v1.json`, with the
`REGENERATE` command of `tests/README.md`.

## Unit 3: human rendering

- `src/parse_human/admin_order_info.rs`: `render_admin_order_info(&AdminOrderInfoData) -> String`. Pure: it takes the outcome data
  and returns text, with no clock, environment, network or store. Blocks as D9, built with the helpers of `parse_human/table.rs`;
  every string from the node goes through `escape_controls`. A `null` time prints as `-`.
- `src/parse_human/mod.rs`: the module and its re-export.

## Tests First (constitution V)

Each fails before its change.

- **Unit, `src/outcome/admin_order_info.rs`**: a full `OrderInfo` maps to the contract's fields (amounts as decimal strings, times
  in both forms, a missing time as `null` in both); a Cashu order has `cashu_escrow` and no `hold_invoice`; an `OrderInfo` deserialized from JSON with
  `"a-state-added-later"` in every enum maps to `unknown` and is not an error (SC-004); the serialized data has no key containing
  `preimage`, `invoice` (other than `hold_invoice`), `token`, `master` or `hash`.
- **Unit, the admin key**: unset and empty are `admin_key_missing`; garbage is `admin_key_invalid`; an `nsec1...` and a 64-character
  hex key parse to the same key; the error's `Debug` and message do not contain the value.
- **Unit, `src/commands/admin/order_info.rs`** (`answer`): the three reply shapes of FR-004 to FR-006, including a snapshot for
  another order id.
- **Schema**: the Story 1 outcome and both refusals validate; an outcome missing a required field does not; `admin_key_missing` is
  in this command's code list and in no other command's.
- **Human**: the four blocks for a full order; a pending order prints the order block only; `unavailable` is printed; a
  `payment_method` with an escape sequence is shown escaped.
- **Process, `tests/admin_order_info_process.rs`**, against `tests/common/fake_mostro.rs`, which learns to answer
  `admin-order-info` (a scripted snapshot, `invalid_pubkey`, `not_found`, a wrong payload, silence):
  - Story 1 and Story 2 in JSON and in human mode; the recorded request is in the `order` wrapper, carries the order id, a request
    id and no payload, and is authored by the admin key;
  - Story 3: `node_refused` / `invalid_pubkey`, exit 1; `not_found` the same way (D5);
  - Story 4: exit 1 with `admin_key_missing`, no connection recorded, `MCLI_DIR` empty afterwards (SC-003);
  - Story 5: the admin key is the node's key and the reply, not the echo of the request, is returned;
  - a wrong payload is `reply_malformed`; silence is `reply_timeout` within `--reply-timeout`;
  - `--order` missing, repeated or not a UUID is `invalid_arguments`, exit 2;
  - the `ADMIN_NSEC` value appears in neither stream of any case (FR-011).

## Docs

`docs/commands.md` (the command under "Admin / solver dispute management", the JSON-capable list, the two error codes),
`docs/architecture.md` (the request path and the admin-key boundary), `AGENTS.md` (status line), `tests/TEST_SUMMARY.md` and
`tests/README.md` (evidence and counts), `contracts/examples.json` captured from the process tests.

## Gates

`cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked`,
`cargo build --locked`, the schema drift check and the production-line count. Report the CI Windows run when it completes.

## Manual Probe (SC-006)

The Ortsom regtest stack with a `mostrod` built from mostro#1110, the stack's solver key in `ADMIN_NSEC`: ask for an order while a
scenario runs and compare `status` with the node's database and `hold_invoice.state` with `lncli lookupinvoice`; then with the
escrow node paused (`unavailable`) and with a key that is not a solver (`invalid_pubkey`).

## Follow-ups Outside This Repository

- A tool for this command in the MCP server, and a page in the mostro-cli book.
- The protocol repository's page for `admin-order-info`.
