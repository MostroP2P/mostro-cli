# Data Model: Add Invoice

Both commands return the standard envelope (v1) with `meta.publication`, `meta.daemon_acceptance`, `meta.persistence`.

## `order.add-invoice` data

| Field | Meaning |
| --- | --- |
| `order_id` | lowercase UUID |
| `result` | `waiting_seller_payment` (node sent `waiting-seller-to-pay`), `payment_accepted` (`hold-invoice-payment-accepted`), `invoice_updated` (`invoice-updated`) |
| `order` | the order the node returned (`payment_accepted` only), else `null` |
| `trade_pubkey` | public key of the order's trade key |

## `bond.add-invoice` data

| Field | Meaning |
| --- | --- |
| `order_id` | lowercase UUID |
| `result` | `invoice_accepted` |
| `counterparty_share_sats` | amount the node reports for the share, `null` when absent |
| `trade_pubkey` | public key of the order's trade key |

## Outcome table

| Case | Status | Code | Stages (publication / acceptance / persistence) |
| --- | --- | --- | --- |
| accepted, row updated | `ok` | | published / accepted / applied (`bond`: not_attempted) |
| accepted, row update failed (`order`) | `ok` + warning `local_status_stale` | | published / accepted / failed |
| `cant-do(r)` | `error` | `node_refused` + reason `r` | published / refused / not_attempted |
| no reply (`order`) / no reply (`bond`) | `uncertain` | `reply_timeout` | published / unknown / not_attempted |
| unusable or wrong-shaped reply | `uncertain` | `reply_malformed` | published / unknown / not_attempted |
| relay read failure after publish | `uncertain` | `relay_read_failed` | published / unknown / not_attempted |
| unknown order or no key | `error` | `order_unknown` (new) | not_published / not_attempted / not_attempted |
| declined at the prompt | `error` | `order_declined` | not_published |
| argument, configuration, identity, protocol, relay, work failures | `error` | as `order take` | not_published |

Statuses, exits (0 ok, 2 `invalid_arguments`, 1 otherwise) and the stable codes are those of `order take`, plus `order_unknown`.
