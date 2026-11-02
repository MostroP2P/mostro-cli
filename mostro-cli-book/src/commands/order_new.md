# order new

| | |
| --- | --- |
| `command` | `order.new` |
| Kind | Write |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity, a writable store, `--yes` in JSON mode |
| Publishes | One `new-order` request signed with a fresh trade key |
| Local write | Reserves the next trade index; saves the order row with its trade key |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.new.input.json`, `schemas/order.new.output.json` |

Creates an order as **maker**. A `sell` order sells sats for fiat; a `buy` order buys sats.

## Invocation

```text
mostro-cli --output json order new --side buy|sell --currency CODE --fiat-amount N
    [--fiat-amount-max M] --payment-method TEXT [--sats N] [--premium N]
    [--invoice INVOICE] [--expiration-days N] --yes
```

Replaces `neworder` and its short flags and the `100-200` range syntax.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--side` | yes | `buy` or `sell`, any letter case |
| `--currency` | yes | 3 to 6 ASCII letters or digits; uppercased. The node decides which currencies it accepts |
| `--fiat-amount` | yes | Integer > 0. With `--fiat-amount-max` it is the range **minimum** |
| `--fiat-amount-max` | no | Integer > `--fiat-amount`. Makes a **range order**; then `--sats` must be 0 |
| `--payment-method` | yes | Non-empty text, ≤ 200 bytes, no control characters. Comma-separate several methods |
| `--sats` | no | Integer ≥ 0, default 0 = market price at take time |
| `--premium` | no | Integer −100..100 percent over/under market, default 0 |
| `--invoice` | no | ≤ 4096 bytes, no whitespace. A buyer's invoice or Lightning address to be paid; which side may carry one is the node's rule (normally a `buy` order) |
| `--expiration-days` | no | 0..365, default 0 = the node's default |
| `--yes` | **yes in JSON** | Without it: `invalid_arguments`, exit 2, nothing done |

Local checks only catch what is cheap and certain. The node remains the authority
(currency list, amount limits, invoice amount); its refusals come back as `node_refused`.

## Behavior

1. Validate, read configuration and identity, check the store is writable.
2. **Reserve** the next trade index and derive its key.
3. Connect, probe the node (protocol, PoW), publish the request, wait for the reply (15 s).
4. On acceptance, save the order row (with trade key and index) in one transaction.

The trade index is **spent** whenever the request was published and not provably refused,
so a later attempt always uses a fresh key. The CLI never retries.

## Success data: `OrderNewData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `order` | [OrderRecord](../types.md#orderrecord) | The order as the node created it (`status` `pending`, or parked while a bond is due) |
| `next_step` | `wait_for_taker` \| `pay_bond_invoice` | What the maker does next |
| `payment_request` | string or null | The bond invoice to pay; only with `pay_bond_invoice` |
| `trade_index` | decimal string | Index of the order's trade key |
| `trade_pubkey` | hex64 | Public trade key of the order |

| `next_step` | Meaning | Do |
| --- | --- | --- |
| `wait_for_taker` | The order is on the book | Watch `message list` for a taker |
| `pay_bond_invoice` | The node requires a maker bond before publishing the order | Pay `payment_request` with a Lightning wallet; the node publishes the order once paid |

## Outcomes

| Case | status | code | Stages (publication / daemon / persistence) |
| --- | --- | --- | --- |
| Created and saved | `ok` | | published / accepted / applied |
| Bond asked first | `ok` (`pay_bond_invoice`) | | published / accepted / applied |
| Local failure, `--yes` missing | `error` | its code | not_attempted / unknown / not_attempted |
| Every relay refused the event | `error` | `request_rejected` | not_attempted / unknown / not_attempted |
| Refused by the node | `error` | `node_refused` + `reason` | published / rejected / not_attempted |
| Created, row **not** saved | `uncertain` | `order_not_saved` (**data present**) | published / accepted / failed |
| No reply / unusable reply / relay dropped wait | `uncertain` | `reply_timeout`, `reply_malformed`, `relay_read_failed` | published / unknown / applied or failed |
| PoW discovered too late | `uncertain` | `pow_required` | published / unknown / applied or failed |
| Relays answered ambiguously | `uncertain` | `request_rejected` | unknown / unknown / applied or failed |

On `uncertain` with `daemon_acceptance: unknown`, `persistence` refers to the reserved
trade index (spent: `applied`; could not be recorded: `failed`).

**After `uncertain`, do not send `order new` again blindly**: the node may already hold a
live order. Look for it first (`message list`, `order list` with your terms, or
`order info` if you have the id). See [Recovering](../flows/uncertain.md).

## Errors

`invalid_arguments`, `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `internal_error`, `identity_missing`,
`identity_unreadable`, `protocol_unsupported`, `pow_required`, `request_rejected`,
`node_refused`, `order_declined` (human prompt only), `store_unwritable`; uncertain:
`reply_timeout`, `reply_malformed`, `request_rejected`, `relay_read_failed`,
`pow_required`, `order_not_saved`.

## Warnings

`invalid_order_timestamp`, `invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/order_new.md}}

## Next

[Selling as a maker](../flows/sell_maker.md) · [Buying as a maker](../flows/buy_maker.md) ·
[Range orders](../flows/range_orders.md)
