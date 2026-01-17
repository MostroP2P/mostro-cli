# Database Schema

Mostro CLI uses a local SQLite database (`mcli.db`) to store user identity and trade history.

## Table: `users`
Stores the BIP-39 mnemonic and identity information.

| Column | Type | Description |
|--------|------|-------------|
| `i0_pubkey` | `CHAR(64)` | Primary Key. The user's identity pubkey. |
| `mnemonic` | `TEXT` | The 12-word seed phrase. |
| `last_trade_index` | `INTEGER` | The last derived trade key index. |
| `created_at` | `INTEGER` | Timestamp of creation. |

## Table: `orders`
Stores details of orders created or taken by the user.

| Column | Type | Description |
|--------|------|-------------|
| `id` | `TEXT` | Primary Key. Order UUID. |
| `kind` | `TEXT` | "buy" or "sell". |
| `status` | `TEXT` | Current status (pending, active, etc.). |
| `amount` | `INTEGER` | Satoshis. |
| `fiat_code` | `TEXT` | e.g., "USD". |
| `fiat_amount` | `INTEGER` | Fiat units. |
| `trade_keys` | `TEXT` | Hex-encoded private key for this trade. |
| `is_mine` | `BOOLEAN` | True if the user created the order. |
| `created_at` | `INTEGER` | Creation timestamp. |

## Implementation Reference
- `src/db.rs`: Contains the `User` and `Order` structs and SQL queries.
- `src/util/storage.rs`: Helper functions for database interaction.
