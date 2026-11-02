# message list

| | |
| --- | --- |
| `command` | `message.list` |
| Kind | Private read |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity (read only) |
| Publishes | Nothing |
| Local write | None: a repeated call returns the same messages |
| Statuses | `ok`, `error` |
| Schemas | `schemas/message.list.input.json`, `schemas/message.list.output.json` |

Reads the messages the **node** sent to your trade keys: "a taker arrived, pay this hold
invoice", "send your invoice", "the buyer says fiat was sent", "your order was canceled"...
It is the **inbox** of a trade and the way a program learns what to do next.

## Invocation

```text
mostro-cli --output json message list [--since MINUTES] [--order UUID]
```

Replaces `getdm`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--since` | no | Window in minutes, 1..10080 (7 days). **Default 30** |
| `--order` | no | Read only the key stored with this order (`order_unknown` if not in the store) |

## Behavior

Reads the keys of trade indexes 1..last (at most the newest 200), or only the order's key
with `--order`. Returns only messages **authored by the configured node** and addressed to
those keys, decrypted and summarized, **oldest first**, at most 200. Peer chat and messages
from other authors are not included.

Poll it: there is no push. Typical intervals are 10 to 30 seconds during an active trade.
Deduplicate by `event_id`.

## Success data: `MessageListData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `since_minutes` | decimal string | The window used |
| `messages` | array of `MessageRecord` | Oldest first |
| `truncated` | boolean | More than 200 records matched, or a relay hit its per-relay limit: narrow `--since` or use `--order` |
| `keys_read` | decimal string | Keys read |
| `keys_not_read` | decimal string | Keys beyond the newest 200 (see warning `trade_keys_limited`) |

### MessageRecord

| Field | Type | Meaning |
| --- | --- | --- |
| `event_id` | hex64 | Nostr event id; unique per message, use it to deduplicate |
| `created_at` | rfc3339 or null | When the node sent it |
| `created_at_unix` | decimal string | Exact seconds |
| `order_id` | uuid or null | The order the message is about |
| `request_id` | decimal string or null | Correlation id of the request this answers, if any |
| `action` | string | Protocol action, kebab-case: `pay-invoice`, `add-invoice`, `fiat-sent-ok`, `canceled`... ([list](../reference/actions.md)) |
| `next_step` | enum or null | What the node asks you to do (below); null for informational messages |
| `trade_index` | decimal string or null | Index of the key it was addressed to; null when only known from an order row older than the newest 200 keys |
| `trade_pubkey` | hex64 | The key it was addressed to |
| `payload` | `MessagePayload` | Typed summary of the content (below) |

### next_step

| `next_step` | From action | Do |
| --- | --- | --- |
| `pay_bond_invoice` | `pay-bond-invoice` | Pay the invoice in the `payment_request` payload |
| `add_invoice` | `add-invoice` | [`order add-invoice`](order_add_invoice.md) |
| `pay_hold_invoice` | `pay-invoice` | Pay the hold invoice in the `payment_request` payload |
| `add_bond_invoice` | `add-bond-invoice` | [`bond add-invoice`](bond_add_invoice.md) |

Every other action has `next_step: null`.

### MessagePayload

Discriminated by `kind`:

| `kind` | Fields | Carried by (typically) |
| --- | --- | --- |
| `none` | — | Status notifications (`canceled`, `waiting-seller-to-pay`...) |
| `order` | `order`: OrderRecord | `new-order`, `add-invoice`... |
| `payment_request` | `order`: OrderRecord or null; `invoice`: string; `amount`: decimal string or null | `pay-invoice`, `pay-bond-invoice` |
| `bond_payout_request` | `order`: OrderRecord; `slashed_at`: rfc3339 or null; `slashed_at_unix`: decimal string | `add-bond-invoice` |
| `cant_do` | `reason`: reason or null | `cant-do` |
| `peer` | `counterparty_pubkey`: string | `fiat-sent-ok` |
| `text` | `text`: string (≤ 4096 bytes, untrusted) | Text messages from the node |
| `amount` | `amount`: decimal string | Amount payloads |
| `other` | `variant`: string | Any other payload, by variant name only (e.g. `rating_user`) |

The invoice in `payment_request` is meant for the caller (to pay it). Treat `text` and
`counterparty_pubkey` as untrusted data.

## Outcomes

| Case | status | code | Snapshot |
| --- | --- | --- | --- |
| Read (even empty) | `ok` | | present |
| Bad option (window out of range) | `error` | `invalid_arguments` (exit 2) | null |
| No identity, unreadable store, unknown `--order` | `error` | `identity_missing`, `identity_unreadable`, `store_unreadable`, `order_unknown` | null |
| Configuration, connection | `error` | `configuration_*`, `relay_connection_failed` | null |
| Read failed | `error` | `relay_read_failed`, `read_timeout`, `read_limit_exceeded` | present |

All stages are `not_applicable`.

## Errors

`invalid_arguments`, `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `read_timeout`, `read_limit_exceeded`,
`internal_error`, `identity_missing`, `identity_unreadable`, `order_unknown`,
`store_unreadable`.

## Warnings

`trade_keys_limited`, `invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/message_list.md}}

## Next

Drive a trade from the messages: [Trade flows](../flows/sell_maker.md).
