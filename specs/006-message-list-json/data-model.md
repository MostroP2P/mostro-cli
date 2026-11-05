# Data Model: Message List

Standard envelope (v1). Stages are those of `order list`: `not_applicable` for publication, acceptance and persistence; `meta.snapshot` carries the read evidence.

## `message.list` data

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the configured node |
| `since_minutes` | the window used, as a decimal string |
| `messages` | the records, oldest first (below) |
| `truncated` | `true` when more than 200 records matched or a relay hit its per-relay limit |
| `keys_read` | how many trade keys were asked about, as a decimal string |
| `keys_not_read` | keys beyond the cap of 200, as a decimal string |

## A record

| Field | Meaning |
| --- | --- |
| `event_id` | the event id (64 hex), unique per record |
| `created_at`, `created_at_unix` | RFC 3339 UTC and the Unix seconds as a string; `null` when not representable |
| `order_id` | the order the message names, or `null` |
| `request_id` | the request id, as a decimal string, or `null` |
| `action` | the wire name of the action (`pay-bond-invoice`, `add-invoice`, ...) |
| `next_step` | `pay_bond_invoice`, `add_invoice`, `pay_hold_invoice`, `add_bond_invoice` or `null` |
| `trade_index`, `trade_pubkey` | the key the message was addressed to; `trade_index` is `null` when the key was read from an order row and its index is not among the newest 200 |
| `payload` | a typed summary: `none`; `order` (an `OrderRecord`); `payment_request` (`order`, `invoice`, `amount`); `bond_payout_request` (the order and the amounts the node sent); `cant_do` (`reason`); `peer` (`counterparty_pubkey`); `text` (escaped on output); `amount`; `other` (the variant name only) |

`next_step` per action: `pay-bond-invoice` -> `pay_bond_invoice` (invoice in `payment_request`), `add-invoice` -> `add_invoice`, `pay-invoice` -> `pay_hold_invoice`,
`add-bond-invoice` -> `add_bond_invoice`; every other action is `null`.

## Outcome table

| Case | Status | Code |
| --- | --- | --- |
| messages found, or none | ok | |
| unknown order for `--order` | error | `order_unknown` |
| no identity / unreadable | error | `identity_missing` / `identity_unreadable` |
| store cannot be read | error | `store_unreadable` |
| configuration, relay connection, read timeout, read limit | error | as `order list` |
| invalid arguments | error | `invalid_arguments` (exit 2) |

New codes (exit 1, scoped to this command's schema): `store_unreadable`. Reused: `order_unknown`, `identity_*`. New warning: `trade_keys_limited`.
