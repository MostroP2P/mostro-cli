# Data Model: order rate

## `order.rate` data

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the node's public key (hex) |
| `order_id` | lowercase UUID |
| `rating` | the rating the node received, a JSON integer 1 to 5 |
| `trade_pubkey` | the public key of the order's trade key |

## Outcomes

| Case | Status | Code | Stages |
| --- | --- | --- | --- |
| `rate-received` with the rating sent | `ok` | | published / accepted / not_attempted |
| `cant-do(r)` | `error` | `node_refused` + `r` | published / rejected / not_attempted |
| no reply (a side that already rated, a full-privacy counterparty) | `uncertain` | `reply_timeout` | published / unknown / not_attempted |
| unusable reply (another rating, another order, a `cant-do` that names another order or none) | `uncertain` | `reply_malformed` | published / unknown / not_attempted |
| rating outside 1-5, and the other local failures | `error` | as `dispute open` | not_attempted / unknown / not_attempted |
