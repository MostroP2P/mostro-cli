# order list

| | |
| --- | --- |
| `command` | `order.list` |
| Kind | Public read |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`. No identity: the database is never opened |
| Publishes | Nothing (only relay subscriptions, with throw-away keys if a relay asks for NIP-42 auth) |
| Statuses | `ok`, `error` |
| Schemas | `schemas/order.list.input.json`, `schemas/order.list.output.json` |

Reads the node's public order book (NIP-69 order events) from the relays and returns the
orders that match the filters.

## Invocation

```text
mostro-cli --output json order list [--side buy|sell] [--currency CODE] [--status STATUS]
```

Replaces `listorders --kind ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--side` | no | `buy` or `sell`, any letter case. Omitted: both sides |
| `--currency` | no | Uppercased, otherwise used as given (no trimming or validation). Omitted: all currencies |
| `--status` | no | One [status](../reference/statuses.md), any letter case. **Default `pending`** (orders that can be taken). There is no "all" value |

## Behavior

1. Subscribes on every configured relay to order events (kind 38383) authored by the node,
   tagged `z=order`, **from the last 7 days**, at most **50 per relay**.
2. Waits until **every** relay has sent EOSE (15 s budget). One relay failing or not finishing
   fails the whole read; there is never a partial success.
3. Deduplicates events, keeps the **latest** event per order id, *then* applies the filters.
4. Sorts by event time, newest first.

Consequences: an order not updated in the last 7 days is not listed; with many relays and
busy books the 50-per-relay limit can hide older orders (`snapshot.received_events` tells
you how much was seen). `complete_history` is always `false`.

## Success data: `ListData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node read from |
| `filters` | object | The normalized filters actually applied: `side` (`buy`/`sell`/null), `currency` (string or null), `status` (string, never null) |
| `orders` | array of [PublicOrder](../types.md#publicorder) | Possibly empty |

`meta.snapshot` is always present on success; see [Snapshot](../types.md#snapshot).

## Outcomes

| Case | status | code | Snapshot |
| --- | --- | --- | --- |
| Read complete (even if nothing matched) | `ok` | | present |
| Bad option | `error` | `invalid_arguments` (exit 2) | null |
| Configuration | `error` | `configuration_missing`, `configuration_invalid` | null |
| Cannot connect | `error` | `relay_connection_failed` | null |
| Relay failed or not finished | `error` | `relay_read_failed`, `read_timeout` | present |
| Too many events | `error` | `read_limit_exceeded` | present |

All stages are `not_applicable`.

## Errors

`invalid_arguments`, `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `read_timeout`, `read_limit_exceeded`,
`internal_error`. All are safe to retry.

## Warnings

`invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/order_list.md}}

## Next

Pick an order and take it: [order take-sell](order_take_sell.md) for a `sell` order,
[order take-buy](order_take_buy.md) for a `buy` order. Note the book shows only the
public view; [order info](order_info.md) asks the node directly.
