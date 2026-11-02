# order take-buy

| | |
| --- | --- |
| `command` | `order.take-buy` |
| Kind | Write |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity, a writable store, `--yes` in JSON mode |
| Publishes | One `take-buy` request signed with a fresh trade key |
| Local write | Reserves the next trade index; saves the order row |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.take-buy.input.json`, `schemas/order.take-buy.output.json` |

Takes a **buy** order: you become the **seller** of the sats. You will have to lock them by
paying a hold invoice.

## Invocation

```text
mostro-cli --output json order take-buy --order UUID [--amount FIAT] --yes
```

Replaces `takebuy -o ...`. There is no `--invoice`: the seller receives fiat, not sats.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of a `buy` order, any case |
| `--amount` | no | Integer > 0: the fiat amount you sell into a **range** order |
| `--yes` | **yes in JSON** | |

## Behavior

As [order take-sell](order_take_sell.md#behavior).

## Success data: `OrderTakeData`

Same fields as [take-sell](order_take_sell.md#success-data-ordertakedata) with
`side: "buy"`.

| `next_step` | Meaning | Do |
| --- | --- | --- |
| `pay_hold_invoice` | Lock the sats: pay the hold invoice in `payment_request` | Pay it with a Lightning wallet. It stays held (not settled) until you release or the trade is canceled |
| `pay_bond_invoice` | A bonded node wants a taker bond first | Pay `payment_request`; a hold invoice follows (see `message list`) |

## Outcomes

As [order new](order_new.md#outcomes) / [take-sell](order_take_sell.md#outcomes).

## Errors

Same set as [order new](order_new.md#errors).

## Warnings

`invalid_order_timestamp`, `invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/order_take_buy.md}}

## Next

[Selling as a taker](../flows/sell_taker.md)
