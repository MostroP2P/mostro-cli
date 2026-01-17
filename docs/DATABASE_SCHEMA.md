# Database Schema

Mostro CLI uses a local SQLite database (`mcli.db`) to store user identity and trade history.

## Table: `users`

Stores the BIP-39 mnemonic and identity information.

| Column | Type | Description |
| -------- | ------ | ------------- |
| `i0_pubkey` | `CHAR(64)` | Primary Key. The user's identity pubkey. |
| `mnemonic` | `TEXT` | The 12-word seed phrase. |
| `last_trade_index` | `INTEGER` | The last derived trade key index. |
| `created_at` | `INTEGER` | Timestamp of creation. |

## Table: `orders`

Stores details of orders created or taken by the user.

| Column | Type | Description |
| -------- | ------ | ------------- |
| `id` | `TEXT` | Primary Key. Order UUID. |
| `kind` | `TEXT` | "buy" or "sell". |
| `status` | `TEXT` | Current status (pending, active, etc.). |
| `amount` | `INTEGER` | Satoshis amount. |
| `min_amount` | `INTEGER` | Minimum satoshis for range orders. |
| `max_amount` | `INTEGER` | Maximum satoshis for range orders. |
| `fiat_code` | `TEXT` | Fiat currency code (e.g., "USD"). |
| `fiat_amount` | `INTEGER` | Fiat units. |
| `payment_method` | `TEXT` | Payment method name. |
| `premium` | `INTEGER` | Premium percentage (basis points). |
| `trade_keys` | `TEXT` | Hex-encoded private key for this trade. |
| `counterparty_pubkey` | `TEXT` | Pubkey of the other party in the trade. |
| `is_mine` | `BOOLEAN` | True if the user created the order. |
| `buyer_invoice` | `TEXT` | Lightning invoice for the buyer. |
| `request_id` | `INTEGER` | Request ID for tracking messages. |
| `created_at` | `INTEGER` | Creation timestamp. |
| `expires_at` | `INTEGER` | Expiration timestamp. |

## Implementation Reference

- `src/db.rs`: Contains the `User` and `Order` structs and SQL queries.
- `src/util/storage.rs`: Helper functions for database interaction.
