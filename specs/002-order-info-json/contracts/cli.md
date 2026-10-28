# CLI Contract: Order Info

**Status**: Proposed schema v1 addition; the current binary does not implement it. It extends the
[order-list contract](../../001-order-list-json/contracts/cli.md) and does not change it: stream,
exit-code, output-selection and malformed-argument rules there apply unchanged. [Data model](../data-model.md)
defines the wire fields. Outcome examples below are synthetic design fixtures, not captured output.

## Invocation

```text
mostro-cli [ROOT OPTIONS] order info --order UUID [--order UUID ...]
```

- `--order` is required, repeatable, takes a UUID in any letter case and is shown in lowercase. A repeated
  id is requested once. There is no count limit in the CLI; the node refuses what it will not answer.
- Root options as in the pilot, plus `--pow BITS` and `--transport nip44` (both optional), placed before the command.
  `--output human|json` works at any depth as before. `--secret` is accepted and ignored.
- The old `ordersinfo` command and its `--orderids` option are removed and fail as an unknown command, without an alias.
- Needs `MOSTRO_PUBKEY` and `RELAYS` and an existing local identity (`mcli.db`). It never creates one, never
  writes to it and never reserves a trade key.
- One signed request is published, using the identity key. Nothing else is published.

## Streams And Exit Codes

Exactly as the order-list contract. With JSON selected, a controlled outcome is one UTF-8 JSON object plus a
newline on stdout, diagnostics are fixed prose on stderr, no prompt is shown and stdin is not read. Exit 0
for `ok`, 2 for `invalid_arguments`, 1 for every other error. If the document cannot be written the process
exits 1 with one fixed stderr line and appends nothing.

## Outcomes

Success (some ids returned):

```json
{"schema_version":"1","command":"order.info","status":"ok",
 "data":{"node_pubkey":"79be667e...f81798",
         "orders":[{"id":"00000000-0000-4000-8000-000000000001","side":"sell","status":"active",
                    "sats_amount":"15000","fiat_currency":"USD","fiat_amount":"15",
                    "fiat_amount_min":null,"fiat_amount_max":null,"payment_method":"bank transfer",
                    "premium_percent":"1","buyer_trade_pubkey":null,"seller_trade_pubkey":"02c6...",
                    "created_at":"2026-10-16T00:00:00Z","created_at_unix":"1792108800",
                    "expires_at":"2026-10-17T00:00:00Z","expires_at_unix":"1792195200"}],
         "not_returned":["00000000-0000-4000-8000-000000000002"]},
 "error":null,
 "warnings":[{"code":"orders_not_returned","message":"..."}],
 "meta":{"publication":"published","daemon_acceptance":"accepted","persistence":"not_applicable","snapshot":null}}
```

Nothing found (the node's `not_found` answer, or a reply with none of the ids): the same shape with
`"orders":[]`, every requested id in `not_returned`, and the same warning.

Refusal:

```json
{"schema_version":"1","command":"order.info","status":"error","data":null,
 "error":{"code":"node_refused","message":"...","reason":"too_many_requests"},
 "warnings":[],
 "meta":{"publication":"published","daemon_acceptance":"rejected","persistence":"not_applicable","snapshot":null}}
```

Stage values for every outcome are listed in the [data model](../data-model.md#errors). `command` is `order.info`
for every error after the command was resolved, and null only when it could not be.

## Stable Errors

Codes inherited from the pilot keep their meaning (`invalid_arguments`, `configuration_missing`,
`configuration_invalid`, `relay_connection_failed`, `relay_read_failed`, `internal_error`). `output_not_supported` is not
possible for this command. Added for it, all exit 1:

| Code | Condition |
| --- | --- |
| identity_missing | no data directory, `mcli.db` or user row |
| identity_unreadable | the identity store exists but cannot be read or holds no valid mnemonic |
| protocol_unsupported | the node advertises protocol version 1 |
| pow_required | the node's advertised proof of work is above the configured work |
| request_rejected | no relay confirmed accepting the request event |
| reply_timeout | the request was published and no matching reply arrived within 15 seconds |
| node_refused | the node refused the request; `error.reason` is its reason in snake case |
| reply_malformed | a matching reply with an unusable action or payload |

These codes appear only in this command's generated schema. The `order.list` and `schema` documents, and their
checked-in fixtures, keep their pilot code sets.

## Correlation And Limits

The reply is accepted only if it is a kind-14 message authored by the configured node, addressed to the identity key,
not our own request, with the request's identifier and the action `orders` or `cant_do`. Anything else is ignored while
waiting. Budgets: 10 seconds to connect, 3 seconds for the node-info probe (best effort), 15 seconds for the reply,
1 second of cleanup. Nothing is retried and a request is never split or repeated.

## Discovery And Versioning

`schema` lists `order.info` as JSON-capable with generated input and output schemas, and the command reference
`docs/schemas/discovery-v1.json` is regenerated by the same command. `docs/schemas/order-info-v1.json` (the
`order.info` output schema) is added and drift-checked like the listing schema. Adding a command with its own
codes is a compatible change within schema version 1; renaming `order.info` or changing the meaning of an existing
code would not be.

## Migration From `ordersinfo`

| Before | After |
| --- | --- |
| `mostro-cli ordersinfo -o ID -o ID2` | `mostro-cli order info --order ID --order ID2` |
| creates the identity if missing | fails with `identity_missing` |
| waits for the first message from the node | waits for the reply to this request |
| prints "No orders found or unauthorized access" | success with `not_returned` and a warning, and the same words in human mode |

The existing MCP server's `get_order` tool runs `ordersinfo` and stops working against this CLI until it is migrated.
