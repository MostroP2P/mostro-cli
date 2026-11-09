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
| Discovery | `cli/schema.rs`, `cli/input_schema.rs`, `docs/schemas/*.json` | `DiscoveryData` and descriptors, its `global_options` (each global option with its schema property, `default` and `applies_to`, built by `input_schema::global_options` from the Clap root and the registry), Clap-driven input schemas (`cli/input_schema.rs`, which also marks the options that read `-` from stdin), `offline_discovery()`; the checked-in fixtures are its output and are drift-tested |
| Operations | `commands/order/list.rs`, `commands/order/info.rs` | Validate input, call `fetch_msg` (list) or `exchange` (info), map to a typed outcome. No printing, exit, storage or client access; `order info` receives the identity keys as an argument |
| Identity | `identity.rs`, `store/create.rs` | Read-only loader of the local identity key (and, for `identity show`, the last trade index) from `mcli.db` (`Missing`/`Unreadable`); never creates, migrates or writes; `db.rs` delegates its derivation to it. Creating a store is `store/create.rs`, shared by the legacy first run and `identity init` |
| Public reads | `msg/mod.rs`, `msg/receive_msg.rs`, `msg/orders.rs` | `PublicReadConfig` and `ReadLimits`, the private `Session` that owns the Nostr client, the bounded all-relays-EOSE collector (`fetch_msg`) and the shared order filter, decoder and reducer |
| Request and reply | `msg/exchange.rs`, `msg/send_msg.rs`, `msg/reply.rs`, `msg/node_info.rs` | `exchange`: one connection, node-info probe (protocol, work), the request mined after it with the advertised work for its lane up to 20 bits (spec 017), notifications armed before subscribe and publish, one publish, the reply correlated by request id and sender, the reply deadline from `--reply-timeout`, bounded cleanup, no retry. The doors stay private |
| Outcomes | `outcome.rs`, `outcome/codes.rs`, `outcome/order.rs`, `outcome/info.rs`, `outcome/schema.rs` | The v1 envelope, stable error and warning codes with exits, the order DTOs, and the generated Draft 2020-12 output schemas |
| Rendering | `parse_machine.rs`, `parse_human/*` | `emit` serializes fully and writes one line; `render_list` builds the human table. Neither decodes events nor touches the network or database |

Rules that hold today:

- **No effects on the migrated paths.** `schema` and `order list` never open the
  database, read identity files, create `MCLI_DIR`, mutate the environment, install a
  logger from user input or read stdin. The public read resolves only
  `MOSTRO_PUBKEY` and `RELAYS`, flags over environment. `order info` also opens
  `mcli.db` read-only to take the identity key, creates nothing, publishes exactly one
  event and never retries; with no identity it fails before any network access.
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
  (`msg::fetch_msg`), and the order chat of spec 019 (`msg::chat`, behind `order chat`) reads through
  it and publishes through the private door of `msg::send_msg`, but the legacy `getdmuser`,
  `dmtouser` and `disputechat` keep their own transport, and there is no shared
  `send_msg`/`receive_msg` for admin messages.
- `util::events` and `parser/orders.rs` keep delegating helpers for the remaining
  Rust callers of the shared order logic; `msg/orders.rs` holds the one implementation.

### Production Inventory: 2026-10-28

**Status**: recount at `65db631` (T047), replacing the T002 snapshot and the T011 update. It is a measured
inventory, not a completed modularity migration. Physical lines include comments
and blanks. Only a trailing `#[cfg(test)] mod` section and two-line `#[cfg(test)]`
imports are excluded; platform-specific production code is counted. This is not a
general Rust parser or a CI gate; task T050 adds the automated check for migrated files.

Totals: **62 files, 13,124 physical lines, 3,106 test-only lines, 10,018 production lines**.
Four files exceed 400 production lines: `parser/dms.rs` (1,210; 1,189 since spec 019 moved `counterparty_from_trade_pubkeys` to `msg/counterparty.rs`), `db.rs` (704; 629 since spec 009 moved the store creation to `store/create.rs`),
`util/messaging.rs` (556) and `cli/take_dispute.rs` (403). Every file created or
migrated by the pilot is at or below 316 production lines (`cli.rs` is the largest).
T019 found the T002 table inconsistent with the files; this table was produced by one
script over all `src/` files and supersedes it. It is a snapshot from the end of the
pilot: `order info` (spec 002) later added `identity.rs`, `cli/migrated.rs`,
`commands/order/info.rs`, `msg/{exchange,send_msg,reply,node_info}.rs`,
`outcome/{codes,info}.rs` and `parse_human/order_info.rs`, each at or below 400
production lines, and removed `cli/orders_info.rs`; `cli.rs` shrank to 189. The size test
(`production_files_respect_the_size_limit_and_the_frozen_exceptions`) is the current check.

| Source under src/ | Physical | Test-only | Production |
| --- | ---: | ---: | ---: |
| cli.rs | 463 | 147 | 316 |
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
| cli/orders_info.rs | 74 | 0 | 74 |
| cli/rate_user.rs | 80 | 0 | 80 |
| cli/registry.rs | 94 | 0 | 94 |
| cli/restore.rs | 76 | 0 | 76 |
| cli/schema.rs | 436 | 208 | 228 |
| cli/send_admin_dm_attach.rs | 313 | 0 | 313 |
| cli/send_dm.rs | 70 | 0 | 70 |
| cli/send_msg.rs | 173 | 0 | 173 |
| cli/take_dispute.rs | 463 | 60 | 403 |
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
remain, `OpenOptionsExt` in db.rs (moved by spec 009 to the top of `store/create.rs` under `#[cfg(unix)]`, so three remain), `StatusCode` in
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
| db.rs (704, now 629) | Roadmap effects phase: connection/migrations and focused user/order persistence behind the db facade; no storage migration in the listing pilot |
| parser/dms.rs (1,210, now 1,189) | Roadmap effects/messaging phases: decode/correlation to msg, reply-driven writes to commands/apply, presentation to parse_human |
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
  - Declares one submodule per legacy command (`adm_send_dm`, `conversation_key`, `dispute_chat`, `dm_to_user`, `get_dm` (the admin path only), `get_dm_user`, `last_trade_index`, `list_disputes`, `maintenance`, `restore`, `send_dm`, `send_admin_dm_attach`, `take_dispute`, `user_info`; `rate_user` and `send_msg` were removed by specs 013 and 014) plus the shared `invocation`, `args`, `commands`, `context`, `legacy`, `registry` and `schema` modules.
  - Re-exports `Cli`, `Commands`, `OrderCommands`, `Context`, the registry (`lookup`, `REGISTRY`, `JsonSupport`) and the discovery types, so callers keep using `crate::cli::{...}`.
  - `run()`:
    - Parses `args_os()` through `parse_invocation`: help and version print text and exit 0; a rejected invocation exits 2 (a JSON error outcome only when JSON was clearly selected).
    - Handles `schema` and `order list` first, with no configuration, database, logger or RPC work (see the call diagram above); a failure to bind accepted arguments is an `internal_error` outcome.
    - With `--output json` on any other command, answers `output_not_supported` from the registry before anything else runs.
    - Otherwise runs daemon admin-RPC commands through `Commands::run_rpc` before any configuration, database or relay work, or constructs a `Context` with `init_context(&cli)` and dispatches through `Commands::run`.
- **`src/cli/args.rs`**: `Cli`, the top-level `clap` arguments (subcommand, verbosity, `--mostro-pubkey`, relay list, PoW, secret mode, transport, the global `--output human|json` and the global `--reply-timeout <SECONDS>` (5 to 300, default 15, spec 016)), and the `--fiatamount` range parser.
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
- **`src/identity.rs`**, **`src/msg/exchange.rs`** (with `send_msg.rs`, `reply.rs`, `node_info.rs`), **`src/commands/order/info.rs`**, **`src/cli/migrated.rs`**: the read-only identity loader, the correlated request and reply, the typed `order.info` operation, and the extracted dispatch of the migrated commands (discovery, listing, info).
- **`src/store.rs`**, **`src/commands/order/new.rs`**, **`src/cli/order_new.rs`**, **`src/outcome/new.rs`**: the first migrated write. `Store` opens the existing `mcli.db` for writing (never creates it; one operation per open) to peek at the next trade key for the human preview, reserve the next trade (`Store::reserve_trade`, `store/reserve.rs`: the index is recorded in one `BEGIN IMMEDIATE` transaction committed before anything is published, so a run that dies after publishing never reuses the key and concurrent runs never share an index; `Store::release` gives it back by compare-and-swap after a request that never left or a refusal, except one for a stale index, `invalid_trade_index`, which keeps it so the next request carries the one after it) and save the order. Every writer of the index only raises it, the legacy `getlasttradeindex` and order save included. `order_new` is the typed `order.new` operation, `cli/order_new.rs` its dispatch (validation, `--yes`, preview and prompt) and `outcome/new.rs` the validated input and the data. `Envelope::uncertain` is the only constructor of the `uncertain` status.
- **`src/cli/write.rs`**, **`src/commands/order/write.rs`**, **`src/commands/order/take.rs`**, **`src/cli/order_take.rs`**, **`src/outcome/take.rs`**: what the write commands share and the takes. `cli/write.rs` resolves configuration, identity and store, builds the exchange budgets from `--reply-timeout` (`exchange_limits`, used by every migrated command that waits for the node's reply; the other budgets keep their defaults), peeks at the next trade only for the preview, applies the JSON `--yes` rule, asks the question and presents the outcome; the operation reserves its own trade after the answer, right before publishing. `commands/order/write.rs` (`Sent`) maps every way an exchange can end to an outcome and states the trade-index policy once. `order_take` serves both sides: take-sell accepts `add-invoice` or `waiting-seller-to-pay`, take-buy `pay-invoice`; the minimal row for `waiting-seller-to-pay` comes from the request, with no second read. `outcome/take.rs` holds the validated input, the legacy-compatible payload and `OrderTakeData` with its `next_step`; the hold invoice is data and appears nowhere else.
- **`src/commands/order/step.rs`**, **`src/cli/step.rs`**, **`src/outcome/step.rs`**: `order fiat-sent`, `order release` and `order cancel`, one operation parameterised by `StepKind`. Like the invoice commands they sign with the order's stored key and carry no payload, except for a range order the user made with room left (`Store::maker_range`): the maker's own step (`release` of a sell order, `fiat-sent` of a buy order, `StepKind::sends_key`) carries a fresh trade key whose index is reserved, and so recorded as spent, with `Store::reserve_trade` right before publishing, and given back (`Store::release`) after a refusal or a request that never left. The node never records that index itself, so recording it first is what keeps a later order from reusing the child's key. The reply is selected by the request id the node echoes to the sender; the order it names is checked afterwards. After an accepted step the local status follows the reply (`Store::set_status`); a failure is the warning `local_status_stale`; an index that cannot be reserved stops the step with `store_unwritable` before anything is sent. The stage vocabulary is `Stages::Step` (on `uncertain`, persistence is `applied` when a key was sent). `cli/send_msg.rs`, which then served only `dispute`, was removed with it by spec 013.
- **`src/commands/order/sync.rs`**, **`src/cli/sync.rs`**, **`src/outcome/sync.rs`**, **`src/parse_human/sync.rs`**: `order sync`. It reads the inbox like `message list` (`cli/write.rs::resolve_reader`, but with a writable store; `Store::trade_keys`, `msg::inbox::read_inbox`), keeps the `new-order` messages whose order the store does not know (`Store::known_orders`) and saves each one as the user's own with the key of its index (`Store::adopt_order`, which never replaces a row and records the index in the same transaction). It publishes nothing and asks for no confirmation. The stage vocabulary is `Stages::Sync` (publication and acceptance not applicable, persistence says what happened to the rows); a failed save is `uncertain` with the rows already saved. The statement that writes an order row lives in `src/store/save.rs`.
- **`src/commands/order/invoice.rs`**, **`src/cli/invoice.rs`**, **`src/outcome/invoice.rs`**: `order add-invoice` and `bond add-invoice`. They reuse `cli/write.rs` through `resolve_base` (configuration, identity, store; writable only for the order invoice) but reserve no trade: the order's stored key (`Store::order_keys`) signs, no trade index is carried and none is spent. `msg::exchange` can also match an acknowledgement that has no request id on the order it names (`Expect::by_order`), which `bond-invoice-accepted` needs. After an accepted `order add-invoice` the local status follows the reply (`Store::set_status`); a failure to update it is the warning `local_status_stale`. `outcome/invoice.rs` holds the validated input (an order invoice is BOLT-11 or an address, a bond invoice BOLT-11 only), both data types and the two stage vocabularies (`Stages::Update`, `Stages::Delivery`).
- **`src/commands/identity.rs`**, **`src/cli/identity.rs`**, **`src/outcome/identity.rs`**, **`src/parse_human/identity.rs`**, **`src/store/create.rs`**: `identity init` and `identity show` (spec 009), local commands that need no node, relay or Mostro key. `store::create_store` creates the store exactly as the legacy first run (`db::connect`) does, through the same `SCHEMA`, `create_private_db_file` and `initialize`: the directory owner-only, `mcli.db` owner-only with `create_new`, the tables and one user with a fresh mnemonic and no trade index. It refuses an existing `mcli.db` or a leftover companion file (`identity_exists`, nothing touched) and removes a half-created file again. `identity show` reads the key and the last trade index through `identity::load_identity_from`, read-only. Stages: `Stages::Local` (`ExecutionMeta::local`) for `init`, read stages for `show`; the registry marks both `JsonSupport::Read`, as for `order sync`. Neither command reads the mnemonic outside the store layer.
- **`src/commands/order/mine.rs`**, **`src/store/rows.rs`**, **`src/outcome/mine.rs`**, **`src/cli/mine.rs`**, **`src/parse_human/mine.rs`**: `order mine` (spec 011), the order rows of the local store, read-only (`Store::order_rows` keeps only the public half of each key); the index of a row's key is found among the newest 200 derived keys (`Store::trade_keys`, as `message list`).
- **`src/commands/order/held.rs`**, **`src/commands/order/dispute.rs`**, **`src/commands/order/rate.rs`**, **`src/outcome/dispute.rs`**, **`src/outcome/rate.rs`**, **`src/cli/held.rs`**, **`src/parse_human/held.rs`**: `dispute open` (spec 013) and `order rate` (spec 014), which replace the legacy `dispute` and `rate`. `held::send_held` is a request on an order the user holds that spends no trade index: the stored key signs, the reply is selected by request id and must name the order, an accepted reply may set the local status (`dispute` for a dispute; nothing for a rating). The trade steps keep their own operation (a range continuation reserves an index) and share its rule and outcomes for a refusal (`held::refusal`: a `cant-do` is a refusal only when it names the order, otherwise `reply_malformed`) and an unusable reply. The legacy `cli/rate_user.rs`, `cli/send_msg.rs` and `util::run_simple_order_msg` are gone.
- **`src/cli/stdin.rs`**: `--invoice -` (spec 010). The four dispatchers that take an invoice (`cli/invoice.rs`, `cli/order_take.rs`, `cli/order_new.rs`) call `stdin::invoice` right after the JSON `--yes` rule and before their validation: a literal value is kept as it is; `-` without `--yes`, or with a terminal as stdin, is `invalid_arguments` before stdin is read; with `--yes` at most 8 KiB + 1 byte is read and exactly one non-empty trimmed UTF-8 line is required. The prompt never reads stdin on this path.
- **`src/commands/message/list.rs`**, **`src/msg/inbox.rs`**, **`src/cli/message.rs`**, **`src/outcome/messages.rs`** (and `messages/payload.rs`), **`src/parse_human/message.rs`**: `message list`, a read with an identity. `cli/write.rs::resolve_reader` resolves configuration, identity and an existing store (never writable, and without the work and transport settings, which concern only what is sent); `Store::trade_keys` derives the keys of indexes 1 to the recorded last (newest 200) or `Store::order_keys` gives the one of `--order`. `msg::inbox::read_inbox` asks every relay once (kind 14, the node as author, all keys as recipients, the window, 50 per relay) through the bounded read of `order list`, then re-checks author, kind, time and recipient itself and opens each event with the key it names; a filter with no recipient is never sent. `outcome/messages.rs` holds the validated input, the records with the typed `next_step`, and the payload summary (the bond, hold and payout invoices are data). The command writes nothing: it saves no counterparty or solver key, which `getdm` did.
- **`src/store/counterparty.rs`**, **`src/msg/counterparty.rs`**, **`src/commands/order/counterparty.rs`**: the counterparty lookup of
  spec 019 (`order chat`). `find_counterparty` returns the row's `counterparty_pubkey`
  (`Store::counterparty`) as it is when it parses and is not our trade key, with no relay contacted. Otherwise it reads all the node's mail
  to the order's trade key since 0 (`read_inbox`) and takes the key from the newest post-lock message about the order
  (`msg::counterparty::counterparty_in`: `buyer-took-order` or `hold-invoice-payment-accepted` naming both trade keys, or `fiat-sent-ok`
  with its `Peer`), then saves it to the row as a best-effort cache (`Store::set_counterparty`; a failed save is not reported). None found
  is `counterparty_unknown`. `counterparty_from_trade_pubkeys`, which the legacy `persist_counterparty_pubkey` still uses, moved from
  `parser/dms.rs` to `msg/counterparty.rs`.
- **`src/msg/chat.rs`**, **`src/outcome/chat.rs`**, **`src/parse_human/chat.rs`**: the order chat of spec 019.
  `msg::chat::read_chat` is one bounded read through `fetch_msg` of the envelopes the conversation's `K_sign` authored (the window as
  `since`, 200 per relay); `open_lines` opens each only through `unwrap_chat_message` with our trade key and the counterparty's as
  writers, counts a message once by its inner id, orders by (time, id) and keeps the newest 200. `send_chat`, the send step, wraps and
  opens its own envelope first, connects the relays, publishes once through `send_msg::publish`, waits for one confirmation within the read
  budget and lets the other relays answer until the same deadline (`Publishing::settle`); every relay refusing is `request_rejected` with
  nothing published, no confirmation with an ambiguous failure is `request_rejected` with publication `unknown`. `outcome/chat.rs` holds
  the validated input, `ChatRecord` (`from` is `me` or `counterparty`, the text bounded to 4096 bytes) and `ChatData`. Its stage
  vocabulary is `Stages::Chat`, an amendment for this command: publication is `not_applicable` until the send step runs, then `published`,
  `not_attempted` (wrap, connection or every relay refusing) or `unknown`; acceptance and persistence are always `not_applicable` (the
  counterparty key the lookup may cache is not reported). `parse_human/chat.rs` renders the conversation with each text line escaped by
  `escape_untrusted` and indented; the indent does not survive the terminal wrapping a long line, which goes on at column 0.
- **`src/commands/order/chat.rs`**, **`src/cli/order_chat.rs`**: `order chat` (spec 019). The registry marks it `JsonSupport::Read`:
  publication stays `not_applicable` until the send step runs, so a rejected invocation reports read stages. `cli/order_chat.rs` refuses
  JSON `--message` without `--yes` (`write::needs_yes`) first, validates the arguments, resolves configuration, identity and store
  with `cli/write.rs::resolve_reader` (read-only, without the work and transport settings) and presents the outcome with
  `cli/write.rs::finish`; human mode sends without a prompt. `order_chat` takes the order's trade key (`Store::order_keys`, else
  `order_unknown`), the counterparty (`find_counterparty`) and the conversation (`read_chat`), and only then sends the message
  (`send_chat`), so a failed read publishes nothing; the message sent is `data.sent`, never one of `data.messages`.
- **`src/outcome/watch.rs`**, **`src/outcome/watch/cursor.rs`**: the contract of `order watch` (spec 020, #246), pure types that no
  command reaches yet. `WatchInput` validates the window (1 to 10080 minutes, default 30), the cursor given with `--after` and the
  deadline (1 to 86400 seconds; without `--timeout`, 30 in JSON and none in human mode). `Cursor` is the opaque `<since>[:<id>,...]` that
  a JSON call returns and takes back: `advance` sets the time to the newest observed record time, clamped to local now, minus 60 s,
  holds it at a chat message's newest envelope while another envelope of it could still be dated at or after it (`Observed::chat` takes
  the inner time for that), keeps the ids observed at or after it (at most 200, raising the time when more qualify, but never past local
  now minus 60 s, else keeping the newest 200) and never moves back. `ends_trade` says which node actions end the trade,
  `admin-settled` only for the seller. `WatchData` carries the order, the counterparty key or null, the new
  `MessageRecord`s and `ChatRecord`s, `ended`, `truncated` and the next cursor; `watch_rules` are read rules with the pilot's warnings
  only. `TradeRole` (`outcome/messages.rs`) is the user's side from the order row's kind and whether the user made it (`of_order`); #243
  reuses it.
- **`src/msg/watch.rs`**, **`src/msg/watch/state.rs`**: the live subscription of `order watch` (spec 020 units 2 and 3; the records
  and limits types sit in `watch/records.rs` and `watch/limits.rs`); no command reaches it yet and it publishes nothing.
  `Watch::open` connects every relay (`Session::live`), arms the notifications (`reply::arm`;
  `Armed::recv` is the shared wait), then sends each relay one REQ per filter (`ReqTarget::single`): the node's mail to the trade key
  (`inbox::mail_filter`) and, when the counterparty is known, the chat (`chat_filter(K_sign)`), since the cursor's time, 50 stored events
  per relay. The opening replay is strict, as for the reads. Every event under a watch subscription counts toward its replay's stored
  events, opened or not; it is then routed by author to `inbox::open_events` (not for mail already seen) or `chat::open_lines` (the
  window applies to the envelope). `state.rs` (pure) holds new records, one entry each, while any relay replays, counts records once (at
  most 10,000, else `read_limit_exceeded`), skips the start cursor's ids and sets `truncated`. Once live no relay event fails the watch.
  `Session::live` is `Session::connect` with automatic reconnection at a fixed retry (`WatchLimits::retry`, 10 s; both share
  `Session::open`, so `add_relay` stays at one call). On every reconnect the SDK sends again each subscription it still holds, with its
  original filter, but only when the reconnect falls in a later second than the REQ (it keeps both times in whole seconds), hence a retry
  of at least 1 s. On a relay's `Connected` the watch sends again, under a new id, each feed (mail, chat) the relay no longer holds by
  `Relay::subscriptions` (after a CLOSED, or never sent while it was away), and every subscription there starts a replay that holds new
  records until its EOSE, at most 15 s. On a CLOSED the watch also unsubscribes that id, since the SDK keeps a `rate-limited:` one and
  would send it again beside the new id. A live replay cut before EOSE is judged for `truncated` as at EOSE. A relay that refuses while it stays connected is not asked again until it reconnects.
  `follow_chat` adds the chat once the counterparty is learned: a REQ to every relay that is connected, then the wait for their
  replays; a relay that is away gets it when it is back. `next` returns released records oldest first or an empty batch at the deadline;
  `observed` feeds `Cursor::advance`; `close` is bounded (`cleanup_incomplete`).
- **`src/commands/order/watch.rs`**, **`src/parse_human/watch.rs`**: the operation and the human renderer of `order watch` (spec 020
  unit 4). `order_watch` takes the order's trade key and index (`message/list.rs::order_key`, the order
  branch of `message list`), the user's side (`TradeRole::of_order` from `Store::order_rows`, whose `StoredOrder` now carries `is_mine`;
  unknown when it is NULL), the counterparty from `find_counterparty` (`counterparty_unknown` only leaves the chat out), then one
  `Watch`. A node message that names the counterparty while watching (`counterparty_in`) brings the chat in through `follow_chat`, its
  replay in the same update; that key is not saved. `Follow::FirstChange` returns the first new records or an empty update at the
  deadline (one JSON document); `Follow::UntilEnd` hands each update to a sink, the first at once even when empty, until a message about
  the order ends the trade for the user's side (`ends_trade`), the deadline passes or the sink cannot write. The outcome is `ok` with
  the last update and a cursor advanced over everything observed; `Watched::ended_by` names the ending action for the human end line.
  `parse_human/watch.rs` renders the header (order, window start, counterparty or its absence), the node's messages with `message
  list`'s block and the chat with `order chat`'s block (escaped by `escape_untrusted`), the learned counterparty and `truncated`, and
  the end or deadline line.
- **`src/cli/order_watch.rs`**: the dispatch of `order watch` in human mode (spec 020 unit 5), the only output that streams
  (constitution 1.0.1). It validates `WatchArgs`, resolves the reader (`write::resolve_reader(cli, false)`: no work or transport
  setting), then runs `order_watch` with `Follow::UntilEnd` and a sink that renders each update whole (`Shown::First` with the window
  start, then `Shown::Later`, flagged when the counterparty first appears), writes and flushes it; a failed write stops the watch with
  one fixed line (exit 1). It then prints the end or deadline line (exit 0), or the error line. The registry marks `order.watch`
  `JsonSupport::Unsupported`, and the `cli/migrated.rs` arm is guarded by human output, so JSON falls through to
  `output_not_supported`.
- **`src/msg/`**: the public-read boundary: `PublicReadConfig`, `ReadLimits`, the private `Session`, `fetch_msg` (`receive_msg.rs`) and the shared order filter, decoder and reducer (`orders.rs`).
- **`src/parse_machine.rs`**: the JSON output boundary. `emit` serializes an outcome completely before writing one line, replaces an unserializable outcome with `internal_error`, and on a failed write exits 1 without appending anything.
- **`src/parse_human/`**: `render_list` and the table helpers (`table.rs`, shared with the legacy listing); pure functions that return text and escape terminal control characters in public text (`escape_controls`). Chat text, which a person wrote, goes through `escape_untrusted`, which also escapes the Unicode 17.0 format characters (Cf: bidirectional controls, zero-width spaces, tags) except the zero-width non-joiner and joiner (U+200C, U+200D, which emoji sequences and scripts such as Persian need) and the line and paragraph separators (Zl, Zp).
- **Command registry** (`REGISTRY` and `lookup` in `src/cli/registry.rs`): every subcommand's canonical name and JSON support. `run()` uses it to name the command in JSON rejections before RPC routing or context initialization.

- **Peer / solver chat** (`src/cli/order_chat.rs`, `src/cli/dm_to_user.rs`, `src/cli/get_dm_user.rs`, `src/cli/dispute_chat.rs`):
  - Protocol#52 kind-14 envelope: `derive_chat_keys` splits `K_conv` / `K_sign`; `wrap_chat_message` / `unwrap_chat_message` encrypt to `K_conv` and sign with `K_sign`. Used by `dmtouser`, `getdmuser`, `disputechat`, and `sendadmindmattach`, each with its own transport, and by `msg/chat.rs` for the migrated order chat (spec 019).
  - Attachment encryption still uses ECDH (`derive_shared_key_bytes`) for the Blossom blob only; the metadata DM is the same kind-14 chat envelope.

### Lightning integration

- **`src/lightning/mod.rs`**
  - Houses Lightning Network–specific helpers used by order flows and invoice handling (exact functions depend on the current version of the file).
  - Typically used by `add_invoice` and the order flows.

### Command modules

Each file in `src/cli/` encapsulates the logic of a specific feature or a group of related commands:

- Order-related: `restore.rs`, `last_trade_index.rs`, `user_info.rs` (`getuserinfo`: reputation for the local identity key; trade-key send + identity proof; `ensure_first_contact_pow` then `wait_for_dm_matching` on `request_id` + `UserInfo`/`CantDo`).
- Disputes and admin: `list_disputes.rs`, `take_dispute.rs`, `adm_send_dm.rs`.
- Messaging: `send_dm.rs`, `dm_to_user.rs`, `get_dm.rs`, `get_dm_user.rs`, `dispute_chat.rs`, `send_admin_dm_attach.rs`, `conversation_key.rs`.

Each legacy module exports an `execute_*` function that `Commands::run` in `src/cli/legacy.rs` calls. `order list`, `order info`, `order new`, `order take-sell`, `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `order mine`, `order rate`, `order chat`, `order watch`, `dispute open`, `bond add-invoice`, `message list`, `identity init`, `identity show` and `schema` are not in this group: they are migrated and live in `commands/`, `cli/migrated.rs`, `cli/order_new.rs`, `cli/order_take.rs`, `cli/invoice.rs`, `cli/step.rs`, `cli/sync.rs`, `cli/message.rs`, `cli/identity.rs`, `cli/mine.rs`, `cli/held.rs`, `cli/order_chat.rs`, `cli/order_watch.rs` and `cli/schema.rs`.

### Typical flow: creating a new order

1. User runs `mostro-cli order new --side ... --yes` (JSON mode requires `--yes`).
2. `clap` parses `OrderCommands::New(NewArgs)`; `cli::migrated::run` hands it to `cli/order_new.rs`, with no legacy `Context`.
3. `OrderNewInput::validate` checks the values, then configuration, the read-only identity (`identity.rs`) and the store (`Store::ensure_writable`; `next_trade` only for the human preview) are resolved, all before anything is published.
4. Human mode shows the preview and asks; JSON never prompts.
5. `commands::order::new::order_new` reserves the next trade (`Store::reserve_trade`: the trade index is recorded before publishing), sends one `new-order` through `msg::exchange`, signed by the trade key under the identity seal, and maps the reply: the order row is saved (`store.rs`); a refusal or a request that never left gives the index back, except a refusal for a stale index (`invalid_trade_index`), which keeps it; a bond request, silence, an unusable reply or a failed save keep it spent and, when the node may hold an order, end `uncertain`.
6. The typed outcome is rendered by `parse_human::order_new` or `parse_machine::emit`.

### Typical flow: taking an order

1. User runs `mostro-cli order take-sell|take-buy --order <uuid> [--invoice ...] [--amount ...] --yes` (JSON mode requires `--yes`).
2. `clap` parses `OrderCommands::TakeSell` or `TakeBuy`; `cli::migrated::run` hands it to `cli/order_take.rs`, with no legacy `Context`.
3. `OrderTakeInput::validate`, then `cli/write.rs` resolves configuration, the read-only identity and the store (peeking at the next trade only for the human preview), all before anything is published.
4. Human mode shows the preview (never the invoice) and asks; JSON never prompts.
5. `commands::order::take::order_take` sends one take through `msg::exchange` and maps the reply to a `next_step`; the trade index was reserved, and so recorded, right before publishing, and the order (the node's, or the minimal one) is saved after the reply. A refusal or a take that never left gives the index back, except a refusal for a stale index, as for `order new`; a bond request, silence, an unusable reply or a failed save keep it spent and, when the node may hold the take, end `uncertain`.
6. The typed outcome is rendered by `parse_human::order_take` or `parse_machine::emit`; the hold invoice is on stdout only.

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

