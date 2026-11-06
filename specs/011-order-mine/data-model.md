# Data Model: order mine

## `order.mine` data

| Field | Meaning |
| --- | --- |
| `orders` | the rows of the local store, newest first |
| `orders[].id` | lowercase UUID |
| `orders[].side` | `buy` or `sell` |
| `orders[].status` | the local status, as the store records it (one of the order statuses) |
| `orders[].trade_index` | the index of the row's trade key, a decimal string; `null` when the key is not among the newest 200 derived or the row has none |
| `orders[].trade_pubkey` | the public key of the row's trade key, 64 lowercase hex; `null` when the row has no usable key |

## Outcomes

| Case | Status | Code | Stages |
| --- | --- | --- | --- |
| rows read (possibly none) | `ok` (warning `trade_keys_limited` when older keys were not derived and a row's index is unknown) | | not_applicable x3 |
| no data directory, no `mcli.db`, no user row | `error` | `identity_missing` | not_applicable x3 |
| the store or a row cannot be read | `error` | `store_unreadable` | not_applicable x3 |
| invalid arguments | `error` | `invalid_arguments` (exit 2) | not_applicable x3 |
