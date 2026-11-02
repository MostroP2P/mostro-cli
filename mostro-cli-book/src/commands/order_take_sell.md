# order take-sell

| | |
| --- | --- |
| `command` | `order.take-sell` |
| Kind | Write |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity, a writable store, `--yes` in JSON mode |
| Publishes | One `take-sell` request signed with a fresh trade key |
| Local write | Reserves the next trade index; saves the order row |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.take-sell.input.json`, `schemas/order.take-sell.output.json` |

Takes a **sell** order: you become the **buyer** of the sats. The name is the side of the
order taken, not your side.

## Invocation

```text
mostro-cli --output json order take-sell --order UUID [--invoice INVOICE] [--amount FIAT] --yes
```

Replaces `takesell -o ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of a `sell` order, any case |
| `--invoice` | no | Where you want to receive the sats: a BOLT-11 invoice or a Lightning address, ≤ 4096 bytes, no whitespace. Without it, the node asks for one next (`add_invoice`) |
| `--amount` | no | Integer > 0: the fiat amount you take from a **range** order. The node checks the range |
| `--yes` | **yes in JSON** | |

## Behavior

Like [order new](order_new.md): reserve a trade index, publish one request, wait for the
correlated reply, save the order row with the trade key. The index is spent whenever the
request was published and not provably refused.

If the node answers that the seller must pay first and sends no order
(`wait_for_seller_payment`), the CLI saves a minimal local row (id, side `sell`, status
`waiting-payment`, the amount if given) so later steps find the trade key; run
`order info` to see the full order.

## Success data: `OrderTakeData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `order_id` | uuid | The order taken |
| `side` | `"sell"` | The side of the order taken |
| `next_step` | enum | See below |
| `order` | [OrderRecord](../types.md#orderrecord) or null | The node's order; `null` when the reply carried none |
| `payment_request` | string or null | Invoice to pay; only with `pay_hold_invoice` or `pay_bond_invoice` |
| `trade_index` | decimal string | Index of the trade key used for this take |
| `trade_pubkey` | hex64 | Public trade key |

| `next_step` | Meaning | Do |
| --- | --- | --- |
| `add_invoice` | The node needs your invoice for the sats | [`order add-invoice`](order_add_invoice.md) (the amount is in `order.sats_amount`) |
| `wait_for_seller_payment` | Your invoice was accepted; the seller must lock the sats | Watch `message list` |
| `pay_bond_invoice` | A bonded node wants a taker bond first | Pay `payment_request`; the node continues the take after |
| `pay_hold_invoice` | (Only for take-buy in practice) | — |

## Outcomes

Identical to [order new](order_new.md#outcomes), with `OrderTakeData` as data.

| Case | status | code | Stages |
| --- | --- | --- | --- |
| Taken and saved | `ok` | | published / accepted / applied |
| Local failure, `--yes` missing | `error` | its code | not_attempted / unknown / not_attempted |
| Refused (taken by someone else, wrong amount...) | `error` | `node_refused` + `reason` | published / rejected / not_attempted |
| Accepted, row not saved | `uncertain` | `order_not_saved` (data present) | published / accepted / failed |
| No / unusable reply, relay dropped wait | `uncertain` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` | published / unknown / applied or failed |
| Ambiguous relays | `uncertain` | `request_rejected` | unknown / unknown / applied or failed |

`reply_malformed` also covers replies this CLI does not handle (for example a Cashu escrow).

After `uncertain`: run `order info --order <id>` and look at `buyer_trade_pubkey` and
`status` before taking again.

## Errors

Same set as [order new](order_new.md#errors).

## Warnings

`invalid_order_timestamp`, `invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/order_take_sell.md}}

## Next

[Buying as a taker](../flows/buy_taker.md)
