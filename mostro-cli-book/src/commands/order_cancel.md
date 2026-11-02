# order cancel

| | |
| --- | --- |
| `command` | `order.cancel` |
| Kind | Write (trade step) |
| Who | The maker of a pending order, or either party of a trade |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, identity, the order's local row, `--yes` in JSON mode |
| Publishes | One `cancel` request signed with the order's trade key |
| Local write | Status → `canceled` or `cooperatively-canceled`; nothing for a cancel request |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.cancel.input.json`, `schemas/order.cancel.output.json` |

Cancels an order. Depending on its state the node either cancels it at once, or (in a
running trade) starts a **cooperative cancel** that the other party must confirm with the
same command.

## Invocation

```text
mostro-cli --output json order cancel --order UUID --yes
```

Replaces `cancel -o ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of an order in the local store |
| `--yes` | **yes in JSON** | |

## Behavior

Looks up the trade key, publishes `cancel`, waits for the reply. Never carries a next trade
key, never spends an index.

| `result` | Meaning | `next_step` | Local status |
| --- | --- | --- | --- |
| `canceled` | Canceled outright (e.g. your own pending order) | null | `canceled` |
| `cancel_requested` | You started a cooperative cancel | `wait_for_peer_cancel` | unchanged |
| `cancel_accepted` | The other party had requested it; now canceled by both | null | `cooperatively-canceled` |

## Success data: `StepData`

See [order fiat-sent](order_fiat_sent.md#success-data-stepdata); `next_trade_index` is
always null here.

## Outcomes

As [order fiat-sent](order_fiat_sent.md#outcomes), plus:

| Case | status | Stages |
| --- | --- | --- |
| `cancel_requested` (nothing to record) | `ok` | published / accepted / not_attempted |

## Errors

As [order add-invoice](order_add_invoice.md#errors). A typical refusal reason is
`order_already_canceled`.

## Warnings

`local_status_stale`, `invalid_event_timestamp`, `cleanup_incomplete`. The schema also
lists `trade_index_not_recorded` (shared with the other steps), but `cancel` never sends a
next trade key, so it does not occur.

## Examples

{{#include ../generated/order_cancel.md}}

## Next

[Cancelling](../flows/cancel.md)
