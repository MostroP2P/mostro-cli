## Architecture & Code Structure

This document describes the internal structure of `mostro-cli`, how major modules interact, and the main data/control flows. It is intentionally high-level to stay stable across refactors.

### Entry point and CLI wiring

- **`src/main.rs`**
  - Initializes the async runtime and delegates to `cli::run()`.
  - Very thin; most logic is in `src/cli.rs`.

- **`src/cli.rs`**
  - Declares submodules for each logical command group: `add_invoice`, `adm_send_dm`, `conversation_key`, `dm_to_user`, `get_dm`, `get_dm_user`, `last_trade_index`, `list_disputes`, `list_orders`, `new_order`, `orders_info`, `rate_user`, `restore`, `send_dm`, `send_admin_dm_attach`, `send_msg`, `take_dispute`, `take_order`.
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
    - `messaging`: higher-level DM helpers (gift-wrapped messages, admin keys, **shared-key derivation and custom wraps**).
    - `misc`: small helpers such as `get_mcli_path` and string utilities.
    - `net`: Nostr network connection setup.
    - `storage`: thin storage helpers for orders and DMs.
    - `types`: small shared enums/wrappers.
  - Re-exports commonly used symbols (`create_filter`, `send_dm`, `connect_nostr`, `save_order`, **`derive_shared_keys`, `derive_shared_key_hex`, `keys_from_shared_hex`, `send_admin_chat_message_via_shared_key`**, etc.) so other modules can import from `crate::util` directly.

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
  - `MessageType` (internal to `util`) distinguishes DM/gift-wrap styles.

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

- **Shared-key custom wraps** (`src/util/messaging.rs`):
  - **Sending**: `derive_shared_keys(local_keys, counterparty_pubkey)` yields a `Keys` whose public key is used as the NIP-59 gift-wrap recipient; inner content is a signed text note encrypted with NIP-44 to that pubkey. Used by `dmtouser` and `sendadmindmattach`.
  - **Receiving**: `unwrap_giftwrap_with_shared_key(shared_keys, event)` decrypts with NIP-44 and returns `(content, timestamp, sender_pubkey)`; `fetch_gift_wraps_for_shared_key(client, shared_keys)` fetches Kind::GiftWrap events with `#p` = shared key pubkey and unwraps them. Use when implementing flows that read shared-key DMs.

### Lightning integration

- **`src/lightning/mod.rs`**
  - Houses Lightning Network–specific helpers used by order flows and invoice handling (exact functions depend on the current version of the file).
  - Typically used by `add_invoice`, `new_order`, and `take_order` modules.

### Command modules

Each file in `src/cli/` encapsulates the logic of a specific feature or a group of related commands:

- Order-related: `add_invoice.rs`, `list_orders.rs`, `new_order.rs`, `take_order.rs`, `orders_info.rs`, `rate_user.rs`, `restore.rs`, `last_trade_index.rs`.
- Disputes and admin: `list_disputes.rs`, `take_dispute.rs`, `adm_send_dm.rs`.
- Messaging: `send_dm.rs`, `send_msg.rs`, `dm_to_user.rs`, `get_dm.rs`, `get_dm_user.rs`, `send_admin_dm_attach.rs`, `conversation_key.rs`.

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

