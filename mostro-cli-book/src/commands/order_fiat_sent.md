# order fiat-sent

| | |
| --- | --- |
| `command` | `order.fiat-sent` |
| Kind | Write (trade step) |
| Who | The **buyer**, after paying the fiat |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, identity, the order's local row, `--yes` in JSON mode |
| Publishes | One `fiat-sent` request signed with the order's trade key |
| Local write | Status → `fiat-sent`; for a buy range maker, the next trade index |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.fiat-sent.input.json`, `schemas/order.fiat-sent.output.json` |

Tells the seller, through the node, that the fiat payment was made.

## Invocation

```text
mostro-cli --output json order fiat-sent --order UUID --yes
```

Replaces `fiatsent -o ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of an order in the local store |
| `--yes` | **yes in JSON** | |

## Behavior

Looks up the order's trade key, publishes `fiat-sent`, waits for `fiat-sent-ok`, records
the local status. If you are the **maker of a buy range order** with room left, the request
also carries a fresh trade key at the next index for the child order the node will create
when the seller releases; `next_trade_index` reports it. See
[Range orders](../flows/range_orders.md).

## Success data: `StepData`

Shared with [release](order_release.md) and [cancel](order_cancel.md).

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `order_id` | uuid | The order |
| `result` | enum | `fiat_sent` here |
| `next_step` | enum or null | `wait_for_release` here |
| `trade_pubkey` | hex64 | The order's trade key |
| `next_trade_index` | decimal string or null | Index reserved for a range order's next key; null when none was sent |

`StepData.result` values across the three steps:

| `result` | Step | Node reply | Local status |
| --- | --- | --- | --- |
| `fiat_sent` | fiat-sent | `fiat-sent-ok` | `fiat-sent` |
| `released` | release | `hold-invoice-payment-settled` or `released` | `settled-hold-invoice` |
| `canceled` | cancel | `canceled` | `canceled` |
| `cancel_requested` | cancel | `cooperative-cancel-initiated-by-you` | unchanged |
| `cancel_accepted` | cancel | `cooperative-cancel-accepted` | `cooperatively-canceled` |

`StepData.next_step` values:

| `next_step` | When | Do |
| --- | --- | --- |
| `wait_for_release` | after `fiat_sent` | Wait for the seller; watch `message list` |
| `wait_for_peer_cancel` | after `cancel_requested` | The other party must also run `order cancel` |
| `sync_child_order` | after `released` of a sell range order that sent a next key | Run [`order sync`](order_sync.md) |
| `null` | otherwise | |

## Outcomes

| Case | status | code | Stages |
| --- | --- | --- | --- |
| Accepted, row updated | `ok` | | published / accepted / applied |
| Accepted, row update failed | `ok` + `local_status_stale` | | published / accepted / failed |
| Accepted, next index not recorded (range) | `ok` + `trade_index_not_recorded` | | published / accepted / failed |
| Not in store | `error` | `order_unknown` | not_attempted / unknown / not_attempted |
| Other local failure | `error` | its code | not_attempted / unknown / not_attempted |
| Refused (wrong status, not the buyer...) | `error` | `node_refused` + `reason` | published / rejected / not_attempted |
| No / unusable reply, relay dropped wait | `uncertain` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` | published / unknown / not_attempted, applied or failed |
| Ambiguous relays | `uncertain` | `request_rejected` | unknown / unknown / not_attempted, applied or failed |

On `uncertain`, `persistence` is `not_attempted` unless a range key was sent (then it says
whether the reserved index was recorded). Check `order info`: status `fiat-sent` means it
went through.

## Errors

As [order add-invoice](order_add_invoice.md#errors).

## Warnings

`local_status_stale`, `trade_index_not_recorded`, `invalid_event_timestamp`,
`cleanup_incomplete`.

## Examples

{{#include ../generated/order_fiat_sent.md}}

## Next

[order release](order_release.md) (by the seller)
