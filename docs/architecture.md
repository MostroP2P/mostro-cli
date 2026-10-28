## Architecture & Code Structure

This document describes the internal structure of `mostro-cli`, how major modules interact, and the main data/control flows. It is intentionally high-level to stay stable across refactors.

### Boundaries Implemented By The Order-List Pilot

**Status (2026-10-28)**: the read-only pilot (`order list`, `schema`, global
`--output`, the early JSON guard) is implemented through task T047 of the
[task list](../specs/001-order-list-json/tasks.md); runtime evidence is in
[TEST_SUMMARY.md](../tests/TEST_SUMMARY.md) and covers native Linux with
loopback relays only. Windows runs, CI wiring and requirement reconciliation
(T050-T055) are still open. Everything else in the CLI is legacy code that
coexists with the new modules and has not been migrated. The
[constitution](../.specify/memory/constitution.md) owns the coding, size and
safety rules; the [roadmap](../../plan-aiReadyMostroCli.md) owns the migration
sequence. This section does not restate either.

```text
main ──▶ cli::run ──▶ cli::invocation::parse_invocation   (args_os, Clap, output recovery)
                 │
                 ├─ schema ───────▶ cli::schema::offline_discovery ──▶ print_discovery | parse_machine::emit
                 ├─ order list ───▶ commands::order::list::list_orders
                 │                      └─▶ msg::fetch_msg ─▶ msg::orders (filter, decode, reduce)
                 │                  outcome ──▶ parse_human::render_list | parse_machine::emit
                 ├─ any other command with --output json ──▶ registry lookup ──▶ output_not_supported
                 └─ any other command, human ──▶ run_rpc | init_context ──▶ Commands::run ──▶ cli/*.rs (legacy)
```

| Area | Files | Responsibility |
| --- | --- | --- |
| Parsing and routing | `cli.rs`, `cli/args.rs`, `cli/commands.rs`, `cli/invocation.rs`, `cli/registry.rs` | Clap grammar (`Cli`, `Commands`, `OrderCommands`, `Schema`), fallible `args_os` parsing with `--output` recovery, the canonical-name and JSON-capability registry, dispatch order (discovery, listing, JSON guard, then legacy) and the only stdout/stderr writes of migrated commands |
| Discovery | `cli/schema.rs`, `docs/schemas/*.json` | `DiscoveryData` and descriptors, Clap-driven input schemas, `offline_discovery()`; the checked-in fixtures are its output and are drift-tested |
| Operations | `commands/order/list.rs` | Validate filters, resolve public configuration, call `fetch_msg`, map to a typed outcome. No printing, exit, storage or client access |
| Public reads | `msg/mod.rs`, `msg/receive_msg.rs`, `msg/orders.rs` | `PublicReadConfig` and `ReadLimits`, the private `Session` that owns the Nostr client, the bounded all-relays-EOSE collector (`fetch_msg`) and the shared order filter, decoder and reducer |
| Outcomes | `outcome.rs`, `outcome/order.rs`, `outcome/schema.rs` | The v1 envelope, stable error and warning codes with exits, the order DTOs, and the generated Draft 2020-12 output schemas |
| Rendering | `parse_machine.rs`, `parse_human/*` | `emit` serializes fully and writes one line; `render_list` builds the human table. Neither decodes events nor touches the network or database |

Rules that hold today:

- **No effects on the migrated paths.** `schema` and `order list` never open the
  database, read identity files, create `MCLI_DIR`, mutate the environment, install a
  logger from user input or read stdin. The public read resolves only
  `MOSTRO_PUBKEY` and `RELAYS`, flags over environment.
- **Typed outcomes, pure presentation.** A command returns an `Envelope`; both
  renderers consume the same data. Machine output is one UTF-8 JSON line, written only
  after complete serialization; a failed write exits 1 and appends nothing.
- **Reads are bounded.** The collector arms notifications before the REQ,
  requires EOSE from every deduplicated relay, caps unique events and
  notifications, never retries and never returns partial rows (10 s connect,
  15 s read, 1 s cleanup by default).
- **Early guard.** `--output json` on any other registered command exits 2 with
  `output_not_supported` before RPC routing, environment access or `init_context`.
- **Private client.** The public read's client lives in the private `msg::Session`
  and is not exported; `compile_fail` doctests in `msg/mod.rs` prove it. This is not a
  crate-wide rule: `ephemeral_client` is still `pub(crate)` for legacy `util/net.rs`. What
  the tests do freeze is where `send_event`, `fetch_events`, `subscribe` and `add_relay`
  may appear and how often (`tests/integration_tests.rs`, `source_boundaries`), so no new
  direct transport can be added outside its owner, and the size, import and no-I/O rules
  of the migrated files.

#### Legacy Coexistence

The remaining 32 commands, unchanged apart from the explicit fixes listed in
`TEST_SUMMARY.md`, still run through `Commands::run_rpc` or `init_context` and
`Commands::run` in `cli/legacy.rs`. Their handlers print directly, publish through
`util::messaging`, `util::events`, `cli/dm_to_user.rs` and
`cli/send_admin_dm_attach.rs`, and mix decoding, local writes and display in
`parser/dms.rs`. `listorders` and the root `--mostropubkey` spelling were removed
without aliases; other legacy spellings are unchanged.

- No new database model, table or migration was added; the existing SQLite
  layer is untouched and described in [database.md](database.md).
- The transport migration is **not** complete. There is a single door for public reads
  (`msg::fetch_msg`), but no shared `send_msg`/`receive_msg` for trades, chat or
  admin messages, and no write command returns an outcome.
- `util::events` and `parser/orders.rs` keep delegating helpers for the remaining
  Rust callers of the shared order logic; `msg/orders.rs` holds the one implementation.

### Production Inventory: 2026-10-28

**Status**: recount at `65db631` (T047), replacing the T002 snapshot and the T011 update. It is a measured
inventory, not a completed modularity migration. Physical lines include comments
and blanks. Only a trailing `#[cfg(test)] mod` section and two-line `#[cfg(test)]`
imports are excluded; platform-specific production code is counted. This is not a
general Rust parser or a CI gate; task T050 adds the automated check for migrated files.

Totals: **62 files, 13,124 physical lines, 3,106 test-only lines, 10,018 production lines**.
Four files exceed 400 production lines: `parser/dms.rs` (1,210), `db.rs` (704),
`util/messaging.rs` (556) and `cli/take_dispute.rs` (403). Every file created or
migrated by the pilot is at or below 316 production lines (`cli.rs` is the largest).
T019 found the T002 table inconsistent with the files; this table was produced by one
script over all `src/` files and supersedes it.

| Source under src/ | Physical | Test-only | Production |
| --- | ---: | ---: | ---: |
| cli.rs | 463 | 147 | 316 |
| cli/add_bond_invoice.rs | 106 | 0 | 106 |
| cli/add_invoice.rs | 93 | 0 | 93 |
| cli/adm_send_dm.rs | 37 | 0 | 37 |
| cli/args.rs | 80 | 0 | 80 |
| cli/commands.rs | 316 | 0 | 316 |
| cli/context.rs | 278 | 64 | 214 |
| cli/conversation_key.rs | 31 | 0 | 31 |
| cli/dispute_chat.rs | 51 | 0 | 51 |
| cli/dm_to_user.rs | 79 | 29 | 50 |
| cli/get_dm.rs | 69 | 0 | 69 |
| cli/get_dm_user.rs | 147 | 0 | 147 |
| cli/invocation.rs | 257 | 0 | 257 |
| cli/last_trade_index.rs | 89 | 0 | 89 |
| cli/legacy.rs | 193 | 0 | 193 |
| cli/list_disputes.rs | 21 | 0 | 21 |
| cli/maintenance.rs | 268 | 70 | 198 |
| cli/new_order.rs | 198 | 0 | 198 |
| cli/orders_info.rs | 74 | 0 | 74 |
| cli/rate_user.rs | 80 | 0 | 80 |
| cli/registry.rs | 94 | 0 | 94 |
| cli/restore.rs | 76 | 0 | 76 |
| cli/schema.rs | 436 | 208 | 228 |
| cli/send_admin_dm_attach.rs | 313 | 0 | 313 |
| cli/send_dm.rs | 70 | 0 | 70 |
| cli/send_msg.rs | 173 | 0 | 173 |
| cli/take_dispute.rs | 463 | 60 | 403 |
| cli/take_order.rs | 384 | 140 | 244 |
| cli/user_info.rs | 83 | 0 | 83 |
| commands/mod.rs | 3 | 0 | 3 |
| commands/order/list.rs | 83 | 0 | 83 |
| commands/order/mod.rs | 1 | 0 | 1 |
| db.rs | 882 | 178 | 704 |
| error.rs | 42 | 0 | 42 |
| lib.rs | 13 | 0 | 13 |
| lightning/mod.rs | 13 | 0 | 13 |
| main.rs | 16 | 0 | 16 |
| msg/mod.rs | 308 | 148 | 160 |
| msg/orders.rs | 82 | 0 | 82 |
| msg/receive_msg.rs | 249 | 0 | 249 |
| nip33.rs | 93 | 0 | 93 |
| outcome.rs | 647 | 350 | 297 |
| outcome/order.rs | 261 | 0 | 261 |
| outcome/schema.rs | 561 | 333 | 228 |
| parse_human/mod.rs | 6 | 0 | 6 |
| parse_human/order.rs | 112 | 67 | 45 |
| parse_human/table.rs | 133 | 0 | 133 |
| parse_machine.rs | 210 | 160 | 50 |
| parser/common.rs | 185 | 0 | 185 |
| parser/disputes.rs | 130 | 2 | 128 |
| parser/dms.rs | 1641 | 431 | 1210 |
| parser/mod.rs | 16 | 0 | 16 |
| parser/orders.rs | 124 | 2 | 122 |
| rpc.rs | 547 | 186 | 361 |
| util/events.rs | 425 | 95 | 330 |
| util/messaging.rs | 824 | 268 | 556 |
| util/misc.rs | 83 | 0 | 83 |
| util/mod.rs | 22 | 0 | 22 |
| util/net.rs | 43 | 0 | 43 |
| util/reply.rs | 258 | 168 | 90 |
| util/storage.rs | 71 | 0 | 71 |
| util/types.rs | 18 | 0 | 18 |

#### Import Findings

Rechecked on the files above: the same four function-local production imports
remain, `OpenOptionsExt` in [db.rs](../src/db.rs#L112), `StatusCode` in
[send_admin_dm_attach.rs](../src/cli/send_admin_dm_attach.rs#L72) and
`DirBuilderExt` / `PermissionsExt` in [misc.rs](../src/util/misc.rs#L69). No new
function-local production import was added by the pilot. Move platform-specific
imports to the file top with their platform guards during the owning
migration, not unconditionally on Windows.

Qualified paths still appear in legacy production code instead of imported symbols
(`sqlx::query` in db, `anyhow::anyhow!` in command helpers, `reqwest::get` in
new_order, SDK paths in events and messaging). The migrated files were checked
only for function-local imports, not for every qualified path, so this is not an
assertion that all import rules pass. These are import debt, not a reason to add
duplicate wrappers; use aliases for the distinct SDK/core `Event` and `Kind` types
when their files migrate. Test-module imports inside production files remain;
they are excluded from size, not exempt from cleanup.

#### Planned Ownership Splits

| Oversized file | Planned split and gate |
| --- | --- |
| db.rs (704) | Roadmap effects phase: connection/migrations and focused user/order persistence behind the db facade; no storage migration in the listing pilot |
| parser/dms.rs (1,210) | Roadmap effects/messaging phases: decode/correlation to msg, reply-driven writes to commands/apply, presentation to parse_human |
| util/messaging.rs (556) | Roadmap shared-messaging phase: private session, `send_msg` and `receive_msg`, with focused private helpers |
| cli/take_dispute.rs (403) | Roadmap dispute/admin phase: separate validation, orchestration and rendering into their owning modules |

Freeze these production sections at or below this snapshot until their planned
split. A prerequisite repair that needs more space must first extract the touched
responsibility without changing behavior. Never shorten formatting or hide code in
test modules to meet the limit. The complete refactor must eliminate every legacy
violation.

### Entry point and CLI wiring

- **`src/main.rs`**
  - Initializes the async runtime and delegates to `cli::run()`.
  - Very thin; most logic starts in `src/cli.rs`.

- **`src/cli.rs`** (facade and dispatcher)
  - Declares one submodule per legacy command (`add_bond_invoice`, `add_invoice`, `adm_send_dm`, `conversation_key`, `dispute_chat`, `dm_to_user`, `get_dm`, `get_dm_user`, `last_trade_index`, `list_disputes`, `maintenance`, `new_order`, `orders_info`, `rate_user`, `restore`, `send_dm`, `send_admin_dm_attach`, `send_msg`, `take_dispute`, `take_order`, `user_info`) plus the shared `invocation`, `args`, `commands`, `context`, `legacy`, `registry` and `schema` modules.
  - Re-exports `Cli`, `Commands`, `OrderCommands`, `Context`, the registry (`lookup`, `REGISTRY`, `JsonSupport`) and the discovery types, so callers keep using `crate::cli::{...}`.
  - `run()`:
    - Parses `args_os()` through `parse_invocation`: help and version print text and exit 0; a rejected invocation exits 2 (a JSON error outcome only when JSON was clearly selected).
    - Handles `schema` and `order list` first, with no configuration, database, logger or RPC work (see the call diagram above); a failure to bind accepted arguments is an `internal_error` outcome.
    - With `--output json` on any other command, answers `output_not_supported` from the registry before anything else runs.
    - Otherwise runs daemon admin-RPC commands through `Commands::run_rpc` before any configuration, database or relay work, or constructs a `Context` with `init_context(&cli)` and dispatches through `Commands::run`.
- **`src/cli/args.rs`**: `Cli`, the top-level `clap` arguments (subcommand, verbosity, `--mostro-pubkey`, relay list, PoW, secret mode, transport and the global `--output human|json`), and the `--fiatamount` range parser.
- **`src/cli/commands.rs`**: the `Commands` enum containing all subcommands and their structured arguments, including `Order { List }` and `Schema`.
- **`src/cli/registry.rs`**: `REGISTRY`, one `CommandEntry` per leaf command (typed path, canonical name, JSON capability), and `lookup`.
- **`src/cli/schema.rs`**: offline discovery (see above).
- **`src/cli/legacy.rs`**: `Commands::run_rpc` and `Commands::run`, the temporary dispatch that maps each variant to its `execute_*` handler.
- **`src/cli/context.rs`**: `Context` (Nostr client, keys, trade index, DB pool, optional admin context keys, and Mostro pubkey) and its initialization.
- **`src/cli/invocation.rs`**: `parse_invocation`, which lets Clap parse argv and, on argument errors, recovers the `--output` declaration and resolved command path from Clap metadata. `run()` calls it on `args_os()` before any effect; a rejected invocation exits 2, as a JSON outcome when JSON was clearly selected and otherwise with its fixed `diagnostic()` text on stderr.
  - `init_context()` (legacy commands only):
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
    - `reply`: reply correlation (`select_reply_event`, `ExpectedReply`): whether a kind-14 is Mostro's answer to our request.
    - `storage`: thin storage helpers for orders and DMs.
    - `types`: small shared enums/wrappers.
  - Re-exports commonly used symbols (`create_filter`, `send_dm`, `connect_nostr`, `save_order`, `wait_for_dm`, `wait_for_dm_matching`, `ensure_first_contact_pow`, `PowRequirementUnmet`, etc.) so other modules can import from `crate::util` directly.
  - **Messaging wait helpers** (`messaging.rs`):
    - `wait_for_dm` — first Mostro kind-14 tagged to the trade key (no `request_id` decrypt).
    - `wait_for_dm_matching` — keeps listening until a decryptable reply echoes `expected_request_id` and (optionally) an allowed action; used by `getuserinfo` for `UserInfo` / `CantDo`.
    - Both use `reply::select_reply_event` for the per-event decision.
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

- **`src/outcome.rs`**, **`src/outcome/order.rs`**, **`src/outcome/schema.rs`**: the machine-readable envelope (contract v1), the `order.list` DTOs and the generated output schemas. `Envelope::ok` and `Envelope::error` are the only constructors, so status, data and error agree; error and warning codes carry fixed messages and exit statuses; `ExecutionMeta` holds the write-stage fields and optional snapshot. `PublicOrder` maps explicit public fields only, with money and times as decimal strings; no core or database record is serialized wholesale.
- **`src/commands/order/list.rs`**: `list_orders`, the typed `order.list` operation (validate, resolve public configuration, `fetch_msg`, select and map). It never prints, exits, writes storage or touches a client.
- **`src/msg/`**: the public-read boundary: `PublicReadConfig`, `ReadLimits`, the private `Session`, `fetch_msg` (`receive_msg.rs`) and the shared order filter, decoder and reducer (`orders.rs`).
- **`src/parse_machine.rs`**: the JSON output boundary. `emit` serializes an outcome completely before writing one line, replaces an unserializable outcome with `internal_error`, and on a failed write exits 1 without appending anything.
- **`src/parse_human/`**: `render_list` and the table helpers (`table.rs`, shared with the legacy listing); pure functions that return text and escape terminal control characters in public text.
- **Command registry** (`REGISTRY` and `lookup` in `src/cli/registry.rs`): every subcommand's canonical name and JSON support. `run()` uses it to name the command in JSON rejections before RPC routing or context initialization.

- **Peer / solver chat** (`src/cli/dm_to_user.rs`, `src/cli/get_dm_user.rs`, `src/cli/dispute_chat.rs`):
  - Protocol#52 kind-14 envelope: `derive_chat_keys` splits `K_conv` / `K_sign`; `wrap_chat_message` / `unwrap_chat_message` encrypt to `K_conv` and sign with `K_sign`. Used by `dmtouser`, `getdmuser`, `disputechat`, and `sendadmindmattach`.
  - Attachment encryption still uses ECDH (`derive_shared_key_bytes`) for the Blossom blob only; the metadata DM is the same kind-14 chat envelope.

### Lightning integration

- **`src/lightning/mod.rs`**
  - Houses Lightning Network–specific helpers used by order flows and invoice handling (exact functions depend on the current version of the file).
  - Typically used by `add_invoice`, `new_order`, and `take_order` modules.

### Command modules

Each file in `src/cli/` encapsulates the logic of a specific feature or a group of related commands:

- Order-related: `add_invoice.rs`, `new_order.rs`, `take_order.rs`, `orders_info.rs`, `rate_user.rs`, `restore.rs`, `last_trade_index.rs`, `user_info.rs` (`getuserinfo`: reputation for the local identity key; trade-key send + identity proof; `ensure_first_contact_pow` then `wait_for_dm_matching` on `request_id` + `UserInfo`/`CantDo`).
- Disputes and admin: `list_disputes.rs`, `take_dispute.rs`, `adm_send_dm.rs`.
- Messaging: `send_dm.rs`, `send_msg.rs`, `dm_to_user.rs`, `get_dm.rs`, `get_dm_user.rs`, `dispute_chat.rs`, `send_admin_dm_attach.rs`, `conversation_key.rs`.

Each legacy module exports an `execute_*` function that `Commands::run` in `src/cli/legacy.rs` calls. `order list` and `schema` are not in this group: they are migrated and live in `commands/` and `cli/schema.rs`.

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
  - Add a variant to `Commands` in `src/cli/commands.rs` and an entry to `REGISTRY` in `src/cli/registry.rs` with its canonical name (tests fail if a subcommand is unregistered).
  - For a human-only command, add the `execute_*` function in a `src/cli/*.rs` module and extend the `Commands::run` arm in `src/cli/legacy.rs`.
  - For a machine-capable command, follow the pilot instead: a typed operation under `src/commands/`, DTOs and codes in `src/outcome/`, rendering in `parse_human`/`parse_machine`, public reads through `msg::fetch_msg`, generated schemas in `cli/schema.rs`, then regenerate `docs/schemas/*.json` (the drift tests fail on stale fixtures).
  - Update `docs/commands.md` to keep documentation in sync.

- **New database fields / tables**:
  - Update `connect()` schema creation and migrations in `src/db.rs`.
  - Extend the relevant model structs and methods.
  - Update `docs/database.md`.

- **New protocol/event type**:
  - Extend `util::types::Event` and the relevant parser module in `src/parser/*`.
  - Adjust listing or DM flows as needed.

