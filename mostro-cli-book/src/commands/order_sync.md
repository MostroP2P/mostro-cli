# order sync

| | |
| --- | --- |
| `command` | `order.sync` |
| Kind | Private read with local writes |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity, a writable store |
| Publishes | **Nothing** |
| Local write | Inserts a row for each announced order the store does not know; never changes an existing row |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/order.sync.input.json`, `schemas/order.sync.output.json` |

> **Development branch.** `order sync` is implemented and tested on `develop` but not yet
> in a release. Check that your binary lists `order.sync` in `schema`.

Gives a local row to orders the node announced to your trade keys, so that the commands
that need a row (`order cancel`, `order release`, `order add-invoice`...) work on them.
Its main use is the **child order of a range order**: after each partial trade the node
creates a new order for the rest of the range under your next trade key, and tells you
with a `new-order` message. It also recovers any such order the store lost.

## Invocation

```text
mostro-cli --output json order sync [--since MINUTES]
```

No `--yes`: it publishes nothing and only adds rows.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--since` | no | How far back to read, in minutes, 1..10080 (7 days). **Default 10080** |

## Behavior

1. Derives your trade keys from index 1 to the recorded last trade index (the newest 200).
2. Reads, like `message list`, the node's messages to those keys in the window.
3. For every `new-order` message whose order id is **not** in the store, saves a row with
   the order, the key it was announced to and that key's index, in one transaction per
   order.
4. Everything else (other actions, known orders) is counted in `messages_ignored`.

It is **idempotent**: a second run saves nothing and changes nothing. It never overwrites a
row. A save that fails stops the run as `uncertain`; the rows saved before stay saved.

## Success data: `SyncData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `since_minutes` | decimal string | The window used |
| `adopted` | array of `AdoptedOrder` | Rows saved by this run, oldest first; empty when nothing was due |
| `messages_read` | decimal string | Messages of the node read in the window |
| `messages_ignored` | decimal string | Messages that led to no row (other actions, known orders) |
| `truncated` | boolean | The read may be incomplete (a relay hit its limit, or more than the record cap matched) |
| `keys_read` | decimal string | Trade keys read |
| `keys_not_read` | decimal string | Older keys beyond the newest 200 |

### AdoptedOrder

| Field | Type | Meaning |
| --- | --- | --- |
| `order` | [OrderRecord](../types.md#orderrecord) | The order as announced (a range child: `pending`, min/max = the rest of the range) |
| `trade_index` | decimal string | Index of the key it was announced to |
| `trade_pubkey` | hex64 | That key |

`meta.snapshot` describes the relay read.

## Outcomes

| Case | status | code | Stages (publication / daemon / persistence) |
| --- | --- | --- | --- |
| Rows saved | `ok` | | not_applicable / not_applicable / applied |
| Nothing to save | `ok` | | not_applicable / not_applicable / not_attempted |
| Failed before reading (arguments, configuration, identity, store) | `error` | its code | not_applicable / not_applicable / not_applicable |
| Read failed, or a save was due and the store is unwritable | `error` | its code | not_applicable / not_applicable / not_attempted |
| Some rows saved, then one failed | `uncertain` | `order_not_saved` (**data present**: what was saved) | not_applicable / not_applicable / failed |

On `uncertain`, simply run it again after fixing the store: it is idempotent.

## Errors

`invalid_arguments`, `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `read_timeout`, `read_limit_exceeded`,
`internal_error`, `identity_missing`, `identity_unreadable`, `store_unreadable`,
`store_unwritable`; uncertain: `order_not_saved`.

## Warnings

`trade_keys_limited`, `invalid_order_timestamp`, `invalid_event_timestamp`,
`cleanup_incomplete`.

## Examples

{{#include ../generated/order_sync.md}}

## Next

[Range orders](../flows/range_orders.md)
