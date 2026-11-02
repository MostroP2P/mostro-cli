# The outcome envelope

Every JSON run prints one **envelope**. Its shape is the same for every command; only `data`
(and the snapshot inside `meta`) depends on the command.

```json
{
  "schema_version": "1",
  "command": "order.cancel",
  "status": "ok",
  "data": { "...": "command-specific" },
  "error": null,
  "warnings": [],
  "meta": {
    "publication": "published",
    "daemon_acceptance": "accepted",
    "persistence": "applied",
    "snapshot": null
  }
}
```

## Fields

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string, always `"1"` | Version of this contract. Reject any other value. |
| `command` | string or null | Canonical dotted name of the command that ran (`order.list`, `order.take-sell`, `schema`). `null` only when the invocation could not be resolved to a command. |
| `status` | `"ok"`, `"error"` or `"uncertain"` | The overall result. See [Status and execution stages](stages.md). |
| `data` | object or null | The command's result. An object on `ok`; **always `null` on `error`**; an object or `null` on `uncertain` (whatever the node did answer). |
| `error` | object or null | `null` on `ok`; an [error object](#the-error-object) on `error` and `uncertain`. |
| `warnings` | array | [Warning objects](#warnings), possibly empty, **never null**. At most one per code. |
| `meta` | object | [Execution stages](stages.md) and, for relay reads, the read [snapshot](types.md#snapshot). |

Rules that hold for every document:

- **Every key is always present.** A missing value is `null` (or `[]` for arrays); keys are
  never omitted. The one exception is `error.reason`, present only on `node_refused`.
- **Key order is not significant.** The CLI writes the envelope keys in the order above, but
  the fixture files in this book are sometimes alphabetical. Parse, do not compare text.
- **No extra keys today**, but within version 1 optional fields may be added. Ignore keys you
  do not know.
- `status`, `data` and `error` always agree. You can rely on: `ok ⇒ error == null`,
  `error ⇒ data == null`, `uncertain ⇒ error != null`.

## The error object

```json
{ "code": "node_refused", "message": "The node refused the request.", "reason": "too_many_requests" }
```

| Field | Type | Meaning |
| --- | --- | --- |
| `code` | string | Stable snake_case code. **The only thing to branch on.** See [Error codes](errors.md). |
| `message` | string | Fixed English sentence chosen by the code. For logs and humans. It never contains data from the node, the arguments, keys or URLs. Not stable for matching. |
| `reason` | string, only with `node_refused` | The node's own refusal reason (`cant-do` reason in the protocol), as a snake_case word matching `^[a-z][a-z0-9_]{0,63}$`. Absent when the node gave none or gave something that is not such a word. |

On `uncertain`, `error.code` says *why* the outcome is uncertain (`reply_timeout`,
`order_not_saved`...), not that the operation failed.

## Warnings

```json
"warnings": [
  { "code": "orders_not_returned", "message": "Some requested orders were not returned by the node." }
]
```

| Field | Type | Meaning |
| --- | --- | --- |
| `code` | string | Stable snake_case code. See [Warning codes](warnings.md). |
| `message` | string | Fixed English sentence. Not stable. |

A warning never changes `status`. It can appear on `ok`, `error` and `uncertain`. Repeated
causes are aggregated: three bad timestamps give one `invalid_event_timestamp`.

## meta

| Field | Type | Meaning |
| --- | --- | --- |
| `publication` | enum | Did a signed request leave for the relays? |
| `daemon_acceptance` | enum | Did the Mostro node accept it? |
| `persistence` | enum | Did the local write the outcome called for happen? |
| `snapshot` | object or null | Evidence of a bounded relay read (`order list`, `message list`, `order sync`); `null` otherwise. |

The three stage enums are explained in [Status and execution stages](stages.md).

## data per command

| `command` | `data` type | Page |
| --- | --- | --- |
| `schema` | `DiscoveryData` | [schema](commands/schema.md) |
| `order.list` | `ListData` | [order list](commands/order_list.md) |
| `order.info` | `OrderInfoData` | [order info](commands/order_info.md) |
| `order.new` | `OrderNewData` | [order new](commands/order_new.md) |
| `order.take-sell`, `order.take-buy` | `OrderTakeData` | [take-sell](commands/order_take_sell.md), [take-buy](commands/order_take_buy.md) |
| `order.add-invoice` | `OrderInvoiceData` | [order add-invoice](commands/order_add_invoice.md) |
| `bond.add-invoice` | `BondInvoiceData` | [bond add-invoice](commands/bond_add_invoice.md) |
| `order.fiat-sent`, `order.release`, `order.cancel` | `StepData` | [fiat-sent](commands/order_fiat_sent.md), [release](commands/order_release.md), [cancel](commands/order_cancel.md) |
| `order.sync` | `SyncData` | [order sync](commands/order_sync.md) |
| `message.list` | `MessageListData` | [message list](commands/message_list.md) |

## Validating

The CLI publishes a JSON Schema per command (`mostro-cli --output json schema`, or the
files in `schemas/`). Each output schema is a `oneOf` of up to four branches:

| Branch title | When |
| --- | --- |
| `success` | `status: ok`, the command's data type, the allowed success stages |
| `failure of the resolved command` | `status: error`, the codes this command can emit, the allowed stages |
| `uncertain outcome of the resolved command` | Only for commands that publish writes (and `order sync`) |
| `failure before the command was resolved` | `command: null`, `invalid_arguments` |

`schemas/envelope-v1.json` (written for this book, not generated) accepts any command's
envelope, including legacy rejections, and is handy as a first-pass check in a wrapper.
