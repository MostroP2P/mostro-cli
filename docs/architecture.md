## Architecture & Code Structure

This document describes the internal structure of `mostro-cli`, how major modules interact, and the main data/control flows. It is intentionally high-level to stay stable across refactors.

### Production Inventory: 2026-10-16

**Status**: Measured T002 baseline after T003's small characterization-helper
extractions, not a completed modularity migration. The
[constitution](../.specify/memory/constitution.md) owns the size/import rules;
[pilot tasks](../specs/001-order-list-json/tasks.md) and the
[roadmap](../../mcp/plan-aiReadyMostroCli.prompt.md) own the migration sequence.

The inventory covers all 42 handwritten files under `src/`, enumerated separately
from the six integration-test files. Physical lines include comments and blanks.
Only trailing `#[cfg(test)] mod tests` sections and their attributes are excluded,
plus the three two-line cfg(test) imports in `parser/dms.rs`. The counter checked
that every excluded test module ends at the file's last nonblank line; empty test
modules are excluded too. Platform-specific production code remains counted on
Windows. This snapshot's rule is not a general Rust parser or a future CI gate.

Totals: **8,854 physical lines, 1,411 test-only lines, 7,443 production lines**.
Five files exceed 400 production lines. No oversized production section was
grown by T003; its touched command/query files remain below the limit.

| Source under src/ | Physical | Test-only | Production |
| --- | ---: | ---: | ---: |
| cli.rs | 873 | 63 | 810 |
| db.rs | 873 | 178 | 695 |
| error.rs | 42 | 0 | 42 |
| lib.rs | 8 | 0 | 8 |
| main.rs | 13 | 0 | 13 |
| nip33.rs | 93 | 0 | 93 |
| rpc.rs | 547 | 186 | 361 |
| cli/add_bond_invoice.rs | 106 | 0 | 106 |
| cli/add_invoice.rs | 93 | 0 | 93 |
| cli/adm_send_dm.rs | 37 | 0 | 37 |
| cli/conversation_key.rs | 31 | 0 | 31 |
| cli/dispute_chat.rs | 51 | 0 | 51 |
| cli/dm_to_user.rs | 79 | 29 | 50 |
| cli/get_dm.rs | 69 | 0 | 69 |
| cli/get_dm_user.rs | 147 | 0 | 147 |
| cli/last_trade_index.rs | 89 | 0 | 89 |
| cli/list_disputes.rs | 21 | 0 | 21 |
| cli/list_orders.rs | 74 | 0 | 74 |
| cli/maintenance.rs | 268 | 70 | 198 |
| cli/new_order.rs | 201 | 0 | 201 |
| cli/orders_info.rs | 74 | 0 | 74 |
| cli/rate_user.rs | 80 | 0 | 80 |
| cli/restore.rs | 76 | 0 | 76 |
| cli/send_admin_dm_attach.rs | 313 | 0 | 313 |
| cli/send_dm.rs | 70 | 0 | 70 |
| cli/send_msg.rs | 173 | 0 | 173 |
| cli/take_dispute.rs | 463 | 60 | 403 |
| cli/take_order.rs | 141 | 0 | 141 |
| cli/user_info.rs | 57 | 0 | 57 |
| lightning/mod.rs | 13 | 0 | 13 |
| parser/common.rs | 185 | 0 | 185 |
| parser/disputes.rs | 130 | 2 | 128 |
| parser/dms.rs | 1609 | 411 | 1198 |
| parser/mod.rs | 16 | 0 | 16 |
| parser/orders.rs | 298 | 2 | 296 |
| util/events.rs | 380 | 74 | 306 |
| util/messaging.rs | 823 | 336 | 487 |
| util/misc.rs | 83 | 0 | 83 |
| util/mod.rs | 20 | 0 | 20 |
| util/net.rs | 52 | 0 | 52 |
| util/storage.rs | 65 | 0 | 65 |
| util/types.rs | 18 | 0 | 18 |

#### Import Findings

Four function-local production imports remain: `OpenOptionsExt` in
[db.rs](../src/db.rs#L112), `StatusCode` in
[send_admin_dm_attach.rs](../src/cli/send_admin_dm_attach.rs#L72), and
`DirBuilderExt` / `PermissionsExt` in [misc.rs](../src/util/misc.rs#L69).
Move platform-specific imports to the file top with their platform guards during
the owning migration, not unconditionally on Windows.

Qualified paths also remain in production instead of imported symbols. Examples
include `sqlx::query` in db, `std::error::Error` in error, environment calls in rpc,
order types in nip33 and parser files, `anyhow::anyhow!` throughout command
helpers, `reqwest::get` in new_order, and duration/chrono/SDK paths in events.
Messaging, misc and net retain qualified helper calls as well. These are import
debt, not a reason to introduce duplicate wrappers. Use aliases for the distinct
SDK/core Event and Kind types when their files migrate.

Test-module imports remain inside cli, db, rpc, dm_to_user, maintenance,
take_dispute, dms, events and messaging. DB tests additionally have function-local
atomic/permission imports. They are excluded from production size, not exempt
from import cleanup; dedicated test modules/files can retain access to private
helpers without moving tests into production. Existing facade reexports in
parser/mod.rs and util/mod.rs should be reviewed with their migration rather than
mistaken for command-local imports.

T003 moved the events module's type import above its constants and the integration
test's function-local imports to the file top. The broader findings remain open;
this inventory is not an assertion that all import rules already pass.

#### Planned Ownership Splits

| Oversized file | Planned split and gate |
| --- | --- |
| cli.rs (810) | T011: small facade plus args, command definitions, context initialization and temporary legacy dispatch; split command families further if needed |
| db.rs (695) | Roadmap effects phase: connection/migrations and focused user/order persistence modules behind the db facade; no storage migration in the listing pilot |
| parser/dms.rs (1198) | Roadmap effects/messaging phases: decode/correlation to msg, reply-driven writes to commands/apply, presentation to parse_human |
| util/messaging.rs (487) | Roadmap shared-messaging phase: private Session, send_msg and receive_msg, with focused private helpers as needed |
| cli/take_dispute.rs (403) | Roadmap dispute/admin phase: separate validation, command orchestration and rendering into their owning modules |

Freeze these production sections at or below this snapshot until their planned
split. A prerequisite repair that needs more space must first extract the touched
responsibility without changing behavior. Never shorten formatting or hide code
in test modules to meet the limit. T050 adds automated checks for migrated files;
the complete refactor must eliminate every legacy violation.

### Entry point and CLI wiring

- **`src/main.rs`**
  - Initializes the async runtime and delegates to `cli::run()`.
  - Very thin; most logic is in `src/cli.rs`.

- **`src/cli.rs`**
  - Declares submodules for each logical command group: `add_invoice`, `adm_send_dm`, `conversation_key`, `dispute_chat`, `dm_to_user`, `get_dm`, `get_dm_user`, `last_trade_index`, `list_disputes`, `list_orders`, `new_order`, `orders_info`, `rate_user`, `restore`, `send_dm`, `send_admin_dm_attach`, `send_msg`, `take_dispute`, `take_order`, `user_info`.
  - Defines:
    - `Context`: runtime dependencies required by commands (Nostr client, keys, trade index, DB pool, optional admin context keys, and Mostro pubkey).
    - `Cli`: top-level arguments parsed by `clap` (subcommand, verbosity, Mostro pubkey override, relay list, PoW, secret mode).
    - `Commands`: enum containing all subcommands and their structured arguments.
  - `run()`:
    - Parses CLI args via `Cli::parse()`.
    - Constructs a `Context` using `init_context(&cli)`.
    - Dispatches: `if let Some(cmd) = &cli.command { cmd.run(&ctx).await?; }`.
  - `init_context()`:
    - Sets environment variables from CLI flags (e.g. `MOSTRO_PUBKEY`, `RELAYS`, `POW`, `SECRET`).
    - Connects to SQLite via `db::connect()`.
    - Loads identity and per-trade keys from the local `users` table.
    - Optionally loads `ADMIN_NSEC` into `context_keys` for admin commands.
    - Resolves `MOSTRO_PUBKEY` via CLI flag or environment.
    - Connects to Nostr relays via `util::connect_nostr()`.

### Utilities and shared infrastructure

- **`src/util/mod.rs`**
  - Organizes utility modules:
    - `events`: event filtering and retrieval from Nostr.
    - `messaging`: higher-level DM helpers (kind-14 NIP-44 wraps, wait-for-reply, admin keys, ECDH shared-secret bytes for attachment encryption).
    - `misc`: small helpers such as `get_mcli_path` and string utilities.
    - `net`: Nostr network connection setup.
    - `storage`: thin storage helpers for orders and DMs.
    - `types`: small shared enums/wrappers.
  - Re-exports commonly used symbols (`create_filter`, `send_dm`, `connect_nostr`, `save_order`, `wait_for_dm`, `wait_for_dm_matching`, `ensure_first_contact_pow`, `PowRequirementUnmet`, etc.) so other modules can import from `crate::util` directly.
  - **Messaging wait helpers** (`messaging.rs`):
    - `wait_for_dm` — first Mostro kind-14 tagged to the trade key (no `request_id` decrypt).
    - `wait_for_dm_matching` — keeps listening until a decryptable reply echoes `expected_request_id` and (optionally) an allowed action; used by `getuserinfo` for `UserInfo` / `CantDo`.
    - `ensure_first_contact_pow` — preflight against kind-38385 `pow_first_contact` (via `events::fetch_pow_first_contact_with`) before first-contact sends.

- **`src/util/storage.rs`**
  - `save_order(order, trade_keys, request_id, trade_index, pool)`:
    - Wraps `Order::new` to insert/update an order row.
    - Logs created order IDs.
    - Updates the `User`'s `last_trade_index` in the `users` table.
  - `run_simple_order_msg(command, order_id, ctx)`:
    - Convenience wrapper that forwards to `cli::send_msg::execute_send_msg(...)` for simple order messages (e.g. `FiatSent`, `Release`, `Cancel`, `Dispute`).
  - `admin_send_dm(ctx, msg)`:
    - Uses `util::messaging::get_admin_keys` and `util::send_dm` to send an admin DM via Nostr.

- **`src/util/types.rs`**
  - `Event` enum:
    - Wraps `SmallOrder`, `Dispute`, and a `Message` tuple `(Message, u64, PublicKey)` for use in parsers and event handling.
  - `ListKind` enum:
    - Identifies what is being listed: `Orders`, `Disputes`, `DirectMessagesUser`, `DirectMessagesAdmin`, `PrivateDirectMessagesUser`.

### Database layer

- **`src/db.rs`**
  - Connection:
    - `connect()` creates or opens `mcli.db` in the CLI data directory from `get_mcli_path()`.
    - On first run, it creates the `orders` and `users` tables via raw SQL.
    - On subsequent runs, it applies a small migration to drop legacy `buyer_token`/`seller_token` columns if present.
  - Models:
    - `User`:
      - Represents the local identity (mnemonic, root pubkey, last trade index).
      - Handles creation (`User::new`), loading (`User::get`), updating (`save`), and key derivation helpers (identity keys and per-trade keys using `nip06`).
    - `Order`:
      - Represents cached orders with fields mapped to the `orders` table.
      - Provides `new`, `insert_db`, `update_db`, fluent setters, `save`, `save_new_id`, `get_by_id`, `get_all_trade_keys`, **`get_all_trade_and_counterparty_keys`** (distinct `(trade_keys, counterparty_pubkey)` pairs for orders where both are set), and `delete_by_id`.
  - See `database.md` for schema details.

### Parsers and protocol types

- **`src/parser/*`**
  - Interpret raw Nostr events into higher-level `Event` variants based on `mostro_core` types.
  - Responsibility split:
    - `orders.rs`: parsing order-related events.
    - `disputes.rs`: parsing dispute events.
    - `dms.rs`: parsing direct messages.
    - `common.rs`: shared parsing helpers.
    - `mod.rs`: module glue.

- **Peer / solver chat** (`src/cli/dm_to_user.rs`, `src/cli/get_dm_user.rs`, `src/cli/dispute_chat.rs`):
  - Protocol#52 kind-14 envelope: `derive_chat_keys` splits `K_conv` / `K_sign`; `wrap_chat_message` / `unwrap_chat_message` encrypt to `K_conv` and sign with `K_sign`. Used by `dmtouser`, `getdmuser`, `disputechat`, and `sendadmindmattach`.
  - Attachment encryption still uses ECDH (`derive_shared_key_bytes`) for the Blossom blob only; the metadata DM is the same kind-14 chat envelope.

### Lightning integration

- **`src/lightning/mod.rs`**
  - Houses Lightning Network–specific helpers used by order flows and invoice handling (exact functions depend on the current version of the file).
  - Typically used by `add_invoice`, `new_order`, and `take_order` modules.

### Command modules

Each file in `src/cli/` encapsulates the logic of a specific feature or a group of related commands:

- Order-related: `add_invoice.rs`, `list_orders.rs`, `new_order.rs`, `take_order.rs`, `orders_info.rs`, `rate_user.rs`, `restore.rs`, `last_trade_index.rs`, `user_info.rs` (`getuserinfo`: reputation for the local identity key; trade-key send + identity proof; `ensure_first_contact_pow` then `wait_for_dm_matching` on `request_id` + `UserInfo`/`CantDo`).
- Disputes and admin: `list_disputes.rs`, `take_dispute.rs`, `adm_send_dm.rs`.
- Messaging: `send_dm.rs`, `send_msg.rs`, `dm_to_user.rs`, `get_dm.rs`, `get_dm_user.rs`, `dispute_chat.rs`, `send_admin_dm_attach.rs`, `conversation_key.rs`.

Each module exports an `execute_*` function that `Commands::run` calls. This keeps `src/cli.rs` as a central router while pushing feature logic into focused files.

### Typical flow: creating a new order

1. User runs `mostro-cli neworder ...`.
2. `clap` parses CLI arguments into `Cli` and `Commands::NewOrder { ... }`.
3. `cli::run()` calls `init_context()` to build `Context` (DB, keys, Nostr client, Mostro pubkey).
4. `Commands::run` matches `Commands::NewOrder` and calls `execute_new_order(...)`.
5. The handler:
   - Uses `mostro_core` types to construct an order message.
   - Sends it to the Mostro backend over Nostr via `util::connect_nostr`/messaging helpers.
   - Persists or updates the local representation via `util::save_order` and `db::Order`.

### Extension guidelines

When adding new features:

- **New command**:
  - Add a variant to `Commands` in `src/cli.rs`.
  - Add the corresponding `execute_*` function in a `src/cli/*.rs` module.
  - Extend the `Commands::run` match arm.
  - Update `docs/commands.md` to keep documentation in sync.

- **New database fields / tables**:
  - Update `connect()` schema creation and migrations in `src/db.rs`.
  - Extend the relevant model structs and methods.
  - Update `docs/database.md`.

- **New protocol/event type**:
  - Extend `util::types::Event` and the relevant parser module in `src/parser/*`.
  - Adjust listing or DM flows as needed.

