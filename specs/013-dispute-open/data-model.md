# Data Model: dispute open

## `dispute.open` data

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the node's public key (hex) |
| `order_id` | lowercase UUID |
| `dispute_id` | the dispute id the node created, lowercase UUID |
| `trade_pubkey` | the public key of the order's trade key |

## Outcomes

| Case | Status | Code | Stages (publication / acceptance / persistence) |
| --- | --- | --- | --- |
| `dispute-initiated-by-you`, status saved | `ok` | | published / accepted / applied |
| the same, local status not saved | `ok` + `local_status_stale` | | published / accepted / failed |
| `cant-do(r)` | `error` | `node_refused` + `r` | published / rejected / not_attempted |
| no reply (also a second dispute, which the node answers with silence) | `uncertain` | `reply_timeout` | published / unknown / not_attempted |
| unusable reply (including a `cant-do` that names another order or none) | `uncertain` | `reply_malformed` | published / unknown / not_attempted |
| unknown order, store, configuration, identity, work, relay before sending, declined, no `--yes` in JSON | `error` | as the trade steps | not_attempted / unknown / not_attempted |
