# Data Model: Trade Steps

All three commands return the standard envelope (v1) with `meta.publication`, `meta.daemon_acceptance`, `meta.persistence` (`snapshot` is null).

## Data (all three)

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the node's public key (hex) |
| `order_id` | lowercase UUID |
| `result` | `fiat_sent`, `released`, `canceled`, `cancel_requested`, `cancel_accepted` |
| `next_step` | `wait_for_release` (after `fiat_sent`), `wait_for_peer_cancel` (after `cancel_requested`), `sync_child_order` (after a `released` that sent a next trade key, spec 008), else `null` |
| `trade_pubkey` | public key of the order's trade key |
| `next_trade_index` | decimal string or `null`; the index reserved for the next trade key a range order's step carried (spec 008) |

## Replies mapped

| Step | Node reply | Result | Local status recorded |
| --- | --- | --- | --- |
| `fiat-sent` | `fiat-sent-ok` | `fiat_sent` | `fiat-sent` |
| `release` | `hold-invoice-payment-settled` or `released` | `released` | `settled-hold-invoice` |
| `cancel` | `canceled` | `canceled` | `canceled` |
| `cancel` | `cooperative-cancel-initiated-by-you` | `cancel_requested` | none |
| `cancel` | `cooperative-cancel-accepted` | `cancel_accepted` | `cooperatively-canceled` |

A reply is selected by node author, addressee, allowed action and the echoed request id; the order it names must be the order of the request and
its payload empty (a `peer` for `fiat-sent-ok`), otherwise the outcome is `reply_malformed`.

## Outcome table

| Case | Status | Code | Stages (publication / acceptance / persistence) |
| --- | --- | --- | --- |
| accepted, row updated | `ok` | | published / accepted / applied |
| accepted, nothing to record (cancel request) | `ok` | | published / accepted / not_attempted |
| accepted, row update failed | `ok` + warning `local_status_stale` | | published / accepted / failed |
| range continuation (spec 008): the next trade index cannot be reserved (amended 2026-11-05 by spec 008) | `error` | `store_unwritable` | not_attempted / unknown / not_attempted |
| `cant-do(r)` naming the order | `error` | `node_refused` + reason `r` | published / rejected / not_attempted |
| no reply | `uncertain` | `reply_timeout` | published / unknown / not_attempted |
| unusable reply, including a `cant-do` that names another order or none (amended 2026-11-05 after review) | `uncertain` | `reply_malformed` | published / unknown / not_attempted |
| relay read failure after publish | `uncertain` | `relay_read_failed` | published / unknown / not_attempted |
| unknown order or no key | `error` | `order_unknown` | not_attempted / unknown / not_attempted |
| declined at the prompt | `error` | `order_declined` | not_attempted |
| argument, configuration, identity, protocol, relay, work failures | `error` | as `order add-invoice` | not_attempted |

On `uncertain`, persistence is `applied` when a range continuation sent a key (its index was recorded before publishing) and `not_attempted` when no key was sent (amended 2026-11-05 by spec 008).
