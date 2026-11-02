# order add-invoice

| | |
| --- | --- |
| `command` | `order.add-invoice` |
| Kind | Write |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, identity, the order's **local row**, `--yes` in JSON mode |
| Publishes | One `add-invoice` request signed with the order's trade key |
| Local write | Updates the order's local status; never advances the trade index |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.add-invoice.input.json`, `schemas/order.add-invoice.output.json` |

Gives the node the **buyer's** invoice for the sats, when it asked for one
(`next_step: add_invoice`), or replaces it after a failed payment.

## Invocation

```text
mostro-cli --output json order add-invoice --order UUID --invoice INVOICE --yes
```

Replaces `addinvoice -o ... -i ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of an order in the local store |
| `--invoice` | yes | A BOLT-11 invoice (for exactly the order's `sats_amount`, unless the node says otherwise) or a Lightning address |
| `--yes` | **yes in JSON** | |

## Behavior

Looks the order up in the local `orders` table to get its trade key (`order_unknown` if
missing; nothing sent), publishes the request, waits for the reply, updates the local
status.

## Success data: `OrderInvoiceData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `order_id` | uuid | The order |
| `result` | enum | What the node did, below |
| `order` | [OrderRecord](../types.md#orderrecord) or null | The node's order; present with `payment_accepted`, otherwise null |
| `trade_pubkey` | hex64 | The order's trade key |

| `result` | Node reply | Meaning | Next |
| --- | --- | --- | --- |
| `waiting_seller_payment` | `waiting-seller-to-pay` | Invoice stored; the seller has not locked the sats yet | Wait (watch `message list`) |
| `payment_accepted` | `hold-invoice-payment-accepted` | Invoice stored and the seller had already paid: the order is `active` | Pay the fiat, then `order fiat-sent` |
| `invoice_updated` | `invoice-updated` | A replacement invoice was stored | Wait for the payout |

## Outcomes

| Case | status | code | Stages |
| --- | --- | --- | --- |
| Accepted, row updated | `ok` | | published / accepted / applied |
| Accepted, nothing to record (`invoice_updated`) | `ok` | | published / accepted / not_attempted |
| Accepted, row update failed | `ok` + `local_status_stale` | | published / accepted / failed |
| Order not in store | `error` | `order_unknown` | not_attempted / unknown / not_attempted |
| Other local failure | `error` | its code | not_attempted / unknown / not_attempted |
| Refused | `error` | `node_refused` + `reason` | published / rejected / not_attempted |
| No / unusable reply, relay dropped wait | `uncertain` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` | published / unknown / not_attempted |
| Ambiguous relays | `uncertain` | `request_rejected` | unknown / unknown / not_attempted |

After `uncertain`: `order info --order <id>`; if the status moved past
`waiting-buyer-invoice`, the invoice was taken.

## Errors

`invalid_arguments`, `configuration_*`, `relay_connection_failed`, `relay_read_failed`,
`internal_error`, `identity_*`, `protocol_unsupported`, `pow_required`, `request_rejected`,
`node_refused`, `order_declined` (human prompt only), `store_unwritable`, `order_unknown`;
uncertain: `reply_timeout`, `reply_malformed`, `request_rejected`, `relay_read_failed`,
`pow_required`.

## Warnings

`local_status_stale`, `invalid_order_timestamp`, `invalid_event_timestamp`,
`cleanup_incomplete`.

## Examples

{{#include ../generated/order_add_invoice.md}}

## Next

[Buying as a taker](../flows/buy_taker.md) · [Buying as a maker](../flows/buy_maker.md)
