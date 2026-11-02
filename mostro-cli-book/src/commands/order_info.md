# order info

| | |
| --- | --- |
| `command` | `order.info` |
| Kind | Request (read-only) |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, an existing identity (read only) |
| Publishes | One request signed with the identity key |
| Local write | None; never reserves a trade index |
| Statuses | `ok`, `error` (never `uncertain`: repeating it is safe) |
| Schemas | `schemas/order.info.input.json`, `schemas/order.info.output.json` |

Asks the node for the current state of one or more orders. Returns the private view
(trade keys, expiry) that the public book does not show.

## Invocation

```text
mostro-cli --output json order info --order UUID [--order UUID ...]
```

Replaces `ordersinfo --orderids ...`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes, repeatable | A UUID in any letter case; reported lowercase. A repeated id is asked once. No count limit in the CLI (the node may refuse large requests) |

## Behavior

Validates arguments → reads configuration → reads the identity (read-only) → connects
(10 s) → probes the node's protocol version and PoW (3 s, best effort) → publishes one
request → waits for the reply correlated by request id (15 s) → cleans up (1 s).
Nothing is retried. The node answers only about orders it is willing to show to this
identity; others come back in `not_returned` without a reason.

## Success data: `OrderInfoData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node asked |
| `orders` | array of [OrderRecord](../types.md#orderrecord) | Returned orders, in request order, each once |
| `not_returned` | array of uuid | Requested ids missing from the reply, in request order |

Invariant: every distinct requested id is in exactly one of `orders[].id` and
`not_returned`. The node's `not_found` answer is a **success** with every id in
`not_returned` and the `orders_not_returned` warning.

## Outcomes

| Case | status | code | Stages (publication / daemon / persistence) |
| --- | --- | --- | --- |
| Answered | `ok` | | published / accepted / not_applicable |
| Local failure (arguments, configuration, identity, protocol, PoW) | `error` | its code | not_attempted / unknown / not_applicable |
| Every relay refused the event | `error` | `request_rejected` | not_attempted / unknown / not_applicable |
| A relay failed ambiguously | `error` | `request_rejected` | unknown / unknown / not_applicable |
| No answer, unusable answer, relay dropped the wait | `error` | `reply_timeout`, `reply_malformed`, `relay_read_failed` | published / unknown / not_applicable |
| Refused | `error` | `node_refused` + `reason` | published / rejected / not_applicable |

## Errors

`invalid_arguments`, `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `internal_error`, `identity_missing`,
`identity_unreadable`, `protocol_unsupported`, `pow_required`, `request_rejected`,
`reply_timeout`, `node_refused`, `reply_malformed`.

## Warnings

`orders_not_returned`, `invalid_order_timestamp`, `invalid_event_timestamp`,
`cleanup_incomplete`.

## Examples

{{#include ../generated/order_info.md}}

## Next

`order info` is the tool to **check before retrying** after an `uncertain` write: see
[Recovering from uncertain outcomes](../flows/uncertain.md).
