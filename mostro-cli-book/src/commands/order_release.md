# order release

| | |
| --- | --- |
| `command` | `order.release` |
| Kind | Write (trade step) — **irreversible** |
| Who | The **seller**, after receiving the fiat |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, identity, the order's local row, `--yes` in JSON mode |
| Publishes | One `release` request signed with the order's trade key |
| Local write | Status → `settled-hold-invoice`; for a sell range maker, the next trade index |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.release.input.json`, `schemas/order.release.output.json` |

Settles the seller's hold invoice: the sats go to the buyer. **It cannot be undone.** Only
release after confirming the fiat arrived in your account; `fiat-sent` from the buyer is a
claim, not proof.

## Invocation

```text
mostro-cli --output json order release --order UUID --yes
```

Replaces `release -o ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of an order in the local store |
| `--yes` | **yes in JSON** | An agent must have explicit user consent for this specific order |

## Behavior

Looks up the trade key, publishes `release`, waits for `hold-invoice-payment-settled` or
`released`, records `settled-hold-invoice`. If you are the **maker of a sell range order**
with room left, the request carries a fresh trade key at the next index; the node creates
a child order for the rest of the range and announces it to that key, and the outcome says
`next_step: sync_child_order`.

## Success data: `StepData`

See [order fiat-sent](order_fiat_sent.md#success-data-stepdata). Here `result` is
`released`; `next_step` is `sync_child_order` (range continued) or `null`.

## Outcomes

As [order fiat-sent](order_fiat_sent.md#outcomes).

After `uncertain`, **never release again blindly**. Run `order info --order <id>`: status
`settled-hold-invoice` or `success` means it went through; `fiat-sent` still means it did
not (yet). Only then decide whether to send it again.

## Errors

As [order add-invoice](order_add_invoice.md#errors).

## Warnings

`local_status_stale`, `trade_index_not_recorded`, `invalid_event_timestamp`,
`cleanup_incomplete`.

## Examples

{{#include ../generated/order_release.md}}

## Next

Rate the counterparty (human-only today: `mostro-cli rate`). For a range order:
[order sync](order_sync.md).
