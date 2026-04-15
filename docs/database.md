## Database Schema & Persistence

`mostro-cli` uses a local SQLite database called `mcli.db` to store:

- User identity (mnemonic, root pubkey, last trade index).
- Cached orders and associated metadata.

This file is created under the CLI data directory returned by `util::get_mcli_path()`.

### Connection and initialization

- Implemented in `src/db.rs`:
  - `connect()`:
    - Builds `mcli_db_path = format!("{}/mcli.db", get_mcli_path())`.
    - If the file does not exist:
      - Creates the file.
      - Initializes a `SqlitePool`.
      - Runs a SQL batch that creates `orders` and `users` tables.
      - Generates a fresh 12-word mnemonic, derives the identity keys, and inserts a `User` row.
    - If the file exists:
      - Connects with `SqlitePool::connect(&db_url)`.
      - Runs `migrate_remove_token_columns` to remove legacy `buyer_token` and `seller_token` columns if present.

### Tables

#### `orders`

- **DDL (from `db.rs`)**:

  ```sql
  CREATE TABLE IF NOT EXISTS orders (
      id TEXT PRIMARY KEY,
      kind TEXT NOT NULL,
      status TEXT NOT NULL,
      amount INTEGER NOT NULL,
      min_amount INTEGER,
      max_amount INTEGER,
      fiat_code TEXT NOT NULL,
      fiat_amount INTEGER NOT NULL,
      payment_method TEXT NOT NULL,
      premium INTEGER NOT NULL,
      trade_keys TEXT,
      counterparty_pubkey TEXT,
      is_mine BOOLEAN,
      buyer_invoice TEXT,
      request_id INTEGER,
      created_at INTEGER,
      expires_at INTEGER
  );
  ```

- **Purpose**:
  - Local cache of orders relevant to the CLI user.
  - Stores the core financial parameters plus:
    - Trade keys (secret key hex for the order).
    - Whether the order belongs to this user.
    - Buyer invoice and request id.
    - Timestamps (`created_at`, `expires_at`).

- **Model**: `db::Order`
  - Fields mirror the columns (with `Option` where null is allowed).
  - Key methods:
    - `Order::new(pool, SmallOrder, trade_keys, request_id)`:
      - Derives an ID (from `SmallOrder.id` or a generated UUID).
      - Fills all fields based on a `mostro_core::SmallOrder` and the current trade keys.
      - Attempts `insert_db`; if a uniqueness error is detected, falls back to `update_db`.
    - `insert_db(&self, pool)`:
      - Performs the `INSERT INTO orders (...) VALUES (...)`.
    - `update_db(&self, pool)`:
      - Performs an `UPDATE` when an order already exists.
    - Fluent setters (`set_kind`, `set_status`, `set_amount`, `set_fiat_code`, etc.) for in-memory mutation before saving.
    - `save(&self, pool)`:
      - Updates an existing order row by ID.
    - `save_new_id(pool, id, new_id)`:
      - Updates the primary key for an order.
    - `get_by_id(pool, id)`:
      - Loads a single order (and returns an error if no ID is present).
    - `get_all_trade_keys(pool)`:
      - Returns distinct non-null `trade_keys` for all orders.
    - `get_all_trade_and_counterparty_keys(pool)`:
      - Returns distinct `(trade_keys, counterparty_pubkey)` pairs for orders where both columns are non-null; useful for deriving per-order shared keys when fetching or sending shared-key DMs.
    - `delete_by_id(pool, id)`:
      - Deletes an order row.

- **Usage**:
  - Many command handlers persist or update orders via `util::save_order`, which internally calls `Order::new` and updates `User::last_trade_index`.

#### `users`

- **DDL (from `db.rs`)**:

  ```sql
  CREATE TABLE IF NOT EXISTS users (
      i0_pubkey char(64) PRIMARY KEY,
      mnemonic TEXT,
      last_trade_index INTEGER,
      created_at INTEGER
  );
  ```

- **Purpose**:
  - Persist the local Mostro CLI identity:
    - Root pubkey for the account (`i0_pubkey`).
    - BIP39 mnemonic.
    - Last used trade index (to derive per-trade Nostr keys deterministically).
    - Creation timestamp.

- **Model**: `db::User`
  - Key methods:
    - `User::new(mnemonic, pool)`:
      - Derives the account keys from the mnemonic with `nip06::FromMnemonic` / `nostr_sdk::Keys::from_mnemonic_advanced`.
      - Inserts a `users` row with `i0_pubkey`, `mnemonic`, and `created_at`.
    - `save(&self, pool)`:
      - Updates `mnemonic` and `last_trade_index` for the stored user.
    - `get(pool)`:
      - Fetches the single user row (LIMIT 1).
    - `get_last_trade_index(pool)` / `get_next_trade_index(pool)`:
      - Helpers for working with the trade index counter.
    - `get_identity_keys(pool)`:
      - Re-derives the identity `Keys` from the stored mnemonic.
    - `get_trade_keys(pool, index)`:
      - Derives per-trade keys for a given index using the same mnemonic.
    - `get_next_trade_keys(pool)`:
      - Computes the next index and returns `(trade_keys, trade_index)`.

- **Usage**:
  - `cli::init_context()`:
    - Uses `User::get_identity_keys` and `User::get_next_trade_keys` to create identity and trade key pairs.
  - `util::save_order()`:
    - After saving an order, it loads `User`, sets `last_trade_index`, and calls `save` to persist progress through the keyspace.

### Migrations

- **`migrate_remove_token_columns(pool)`** in `db.rs`:
  - Checks for the presence of `buyer_token` and `seller_token` columns via `pragma_table_info('orders')`.
  - If either exists, attempts to drop them with `ALTER TABLE orders DROP COLUMN ...`.
  - Logs warnings instead of failing hard so older databases can continue working even if some engines do not support the `DROP COLUMN` syntax.

### Helper utilities

- **`util::storage`**:
  - `save_order(order, trade_keys, request_id, trade_index, pool)`:
    - Central place for persisting `Order` and updating the `User` record.
  - `run_simple_order_msg(...)` and `admin_send_dm(...)` are not strictly DB-related but are often used alongside order persistence.

### Extension guidelines

- When adding a new column to `orders` or `users`:
  - Update the `CREATE TABLE` statement in `connect()`.
  - Extend the corresponding struct fields in `Order` or `User`.
  - Update `insert_db`, `update_db`, and `save` statements.
  - Add a migration helper if the change is not backward compatible with existing databases.
  - Update this `database.md` file.

- When introducing a new table:
  - Add a `CREATE TABLE` clause to the initialization block in `connect()`.
  - Create a new model struct with `sqlx::FromRow`.
  - Provide CRUD helpers similar to `Order` and `User`.
  - Document it here for clarity.

