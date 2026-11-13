# CLI Contract: admin order-info

**Status**: Implemented schema v1 addition. It builds on the
[order-info contract](../../002-order-info-json/contracts/cli.md): stream, exit-code, output-selection and malformed-argument rules
there apply unchanged. The outcome examples below are abridged; [examples.json](examples.json) holds complete outcomes, reproduced
from the outcome types and checked against the generated schema by `admin_order_info_examples_conform_and_are_reproduced`.

## Invocation

```text
mostro-cli [ROOT OPTIONS] admin order-info --order UUID
```

- Canonical name `admin.order-info`. `--order` is required, given once, takes a UUID in any letter case and is shown in lowercase.
- Root options as for `order info`: `--mostro-pubkey`, `--relays`, `--pow BITS`, `--transport nip44`, `--reply-timeout`, and
  `--output human|json` at any depth.
- Needs `MOSTRO_PUBKEY` and `RELAYS` (option, environment or config file) and `ADMIN_NSEC` in the environment: the private key of
  the node's admin or of one of its registered solvers. The config file does not take `ADMIN_NSEC`.
- The local identity store is not used: nothing is opened, read or created in the data directory except the config file, read-only.
- One signed request is published, with the admin key as identity and author. Nothing else is published and nothing is retried.

## Data

Success `data`. Amounts and counts are decimal strings; each time is an RFC 3339 UTC string with its Unix seconds beside it in a
`_unix` field, both `null` when the node holds no time. The values in *open enum* fields are the node's own text: the ones listed are
those known today, and a client must accept any other string.

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex | the node that answered |
| `order_id` | UUID | the order asked about |
| `side` | open enum: `buy`, `sell` | order kind |
| `status` | open enum: the node's order statuses, in kebab case | the status stored in the node's database, not the reduced one of the public order event |
| `sats_amount` | decimal | sats amount |
| `fee_sats` | decimal | the node's fee for this trade |
| `routing_fee_sats` | decimal | Lightning routing fee observed when paying the buyer |
| `fiat_currency` | string | fiat code |
| `fiat_amount` | decimal | fiat amount |
| `fiat_amount_min`, `fiat_amount_max` | decimal or null | bounds of a range order |
| `payment_method` | string | as the maker wrote it |
| `premium_percent` | decimal | premium over the spot price |
| `range_parent_id` | UUID or null | the parent order, when this one is a slice of a range order |
| `created_at`, `created_at_unix` | time | when the order was created |
| `taken_at`, `taken_at_unix` | time or null | when it was taken |
| `expires_at`, `expires_at_unix` | time | when it expires |
| `buyer_trade_pubkey`, `seller_trade_pubkey` | hex or null | trade keys of the parties; never a master key |
| `buyer_dispute`, `seller_dispute` | boolean | who opened a dispute |
| `buyer_cooperative_cancel`, `seller_cooperative_cancel` | boolean | who asked for a cooperative cancel |
| `failed_payment` | boolean | the node's `failed_payment` flag for the payment to the buyer |
| `payment_attempts` | decimal | payment attempts counted so far |
| `hold_settled_at`, `hold_settled_at_unix` | time or null | when the seller's hold invoice was settled |
| `hold_invoice` | object or null | the seller's hold invoice; `null` when the order has no hold invoice hash |
| `hold_invoice.state` | open enum: `open`, `accepted`, `settled`, `canceled`, `not-found`, `unavailable`, `unknown` | its state at the Lightning node |
| `hold_invoice.held_at`, `hold_invoice.held_at_unix` | time or null | when the seller paid it |
| `cashu_escrow` | object or null | the Cashu escrow of a Cashu order |
| `cashu_escrow.mint_url` | string | the mint holding it |
| `cashu_escrow.locked_at`, `cashu_escrow.locked_at_unix` | time or null | when it was locked |
| `payout` | object or null | the payment to the buyer; `null` unless one is in flight |
| `payout.state` | open enum: `in-flight`, `succeeded`, `failed`, `not-found`, `unavailable`, `unknown` | its state at the Lightning node |
| `payout.failure_reason` | open enum or null: `timeout`, `no-route`, `error`, `incorrect-payment-details`, `insufficient-balance`, `canceled`, `unknown` | why it failed |
| `payout.resolved_at`, `payout.resolved_at_unix` | time or null | when it reached its final state |
| `bonds` | array | the bond rows the node reports for the order, oldest first; empty when there is none |
| `bonds[].role` | open enum: `maker`, `taker`, `unknown` | who posted it |
| `bonds[].amount_sats` | decimal | amount of the row |
| `bonds[].state` | open enum: `requested`, `locked`, `released`, `pending-payout`, `slashed`, `forfeited`, `failed`, `unknown` | state of the row |
| `bonds[].slash_reason` | open enum or null: `lost-dispute`, `timeout`, `unknown` | why it was slashed |
| `bonds[].kind` | open enum: `bond`, `slice-slash`, `maker-refund`, `unknown` | the bond itself, a maker slash on a slice of a range order, or the remainder returned to the maker |
| `bonds[].root_order_id` | UUID or null | the root order, when the row is the maker bond of the range order this slice belongs to |

Two states are not what the Lightning node said. `not-found` means it has no record of that hash; for a payout this is also the
short time between the node recording the payment and sending it. `unavailable` means the node could not ask its Lightning node. In
both cases the rest of `data` is answered as usual. `unknown` is a value the node sent that its protocol library could not name.

## Outcomes

Success:

```json
{"schema_version":"1","command":"admin.order-info","status":"ok",
 "data":{"node_pubkey":"79be667e...f81798","order_id":"00000000-0000-4000-8000-000000000001",
         "side":"sell","status":"settled-hold-invoice","sats_amount":"100000","fee_sats":"600","routing_fee_sats":"0",
         "fiat_currency":"EUR","fiat_amount":"50","fiat_amount_min":null,"fiat_amount_max":null,
         "payment_method":"SEPA","premium_percent":"1","range_parent_id":null,
         "created_at":"2026-10-16T00:00:00Z","created_at_unix":"1792108800",
         "taken_at":"2026-10-16T00:01:40Z","taken_at_unix":"1792108900",
         "expires_at":"2026-10-17T00:00:00Z","expires_at_unix":"1792195200",
         "buyer_trade_pubkey":"02c6...","seller_trade_pubkey":"03f1...",
         "buyer_dispute":false,"seller_dispute":false,
         "buyer_cooperative_cancel":false,"seller_cooperative_cancel":false,
         "failed_payment":true,"payment_attempts":"2",
         "hold_settled_at":"2026-10-16T00:04:10Z","hold_settled_at_unix":"1792109050",
         "hold_invoice":{"state":"settled","held_at":"2026-10-16T00:03:20Z","held_at_unix":"1792109000"},
         "cashu_escrow":null,"payout":null,
         "bonds":[{"role":"taker","amount_sats":"1000","state":"locked","slash_reason":null,"kind":"bond","root_order_id":null}]},
 "error":null,"warnings":[],
 "meta":{"publication":"published","daemon_acceptance":"accepted","persistence":"not_applicable","snapshot":null}}
```

Refusal:

```json
{"schema_version":"1","command":"admin.order-info","status":"error","data":null,
 "error":{"code":"node_refused","message":"...","reason":"invalid_pubkey"},
 "warnings":[],
 "meta":{"publication":"published","daemon_acceptance":"rejected","persistence":"not_applicable","snapshot":null}}
```

`reason` is `invalid_pubkey` when the key is neither the node's nor a registered solver's, and `not_found` when no order has that
id. A node that does not know the action answers nothing: the outcome is `reply_timeout`.

## Stable Errors

Those of `order info` except the identity codes: `invalid_arguments` (exit 2), `configuration_missing`, `configuration_invalid`,
`relay_connection_failed`, `relay_read_failed`, `protocol_unsupported`, `pow_required`, `request_rejected`, `reply_timeout`,
`node_refused`, `reply_malformed`, `internal_error`. `output_not_supported` is not possible for this command. Added for it, both
exit 1 and both before any relay is contacted:

| Code | Condition |
| --- | --- |
| admin_key_missing | `ADMIN_NSEC` is not set, or is empty |
| admin_key_invalid | `ADMIN_NSEC` is set and is not a private key (`nsec1...` or 64 hex characters) |

These two codes appear only in this command's generated schema.

Warnings: `invalid_order_timestamp` (a time of the reply cannot be represented and is `null`) and `cleanup_incomplete`, both as
in `order info`.

## Human Output

Stdout, for a success: the order block, then the escrow, payout and bonds blocks when the order has them. Text received from the
node (`payment_method`, `fiat_currency`, `mint_url`, and any open-enum value) is shown with control characters escaped. A failure
prints nothing on stdout and one line on stderr, with the outcome's exit status. The key in `ADMIN_NSEC` is never shown.

## Correlation And Limits

As `order info`: the reply is accepted only if it is a kind-14 message authored by the configured node, addressed to the admin key,
not our own request, with the request's identifier and the action `admin-order-info` or `cant-do`. Budgets: 10 seconds to connect,
3 seconds for the node-info probe (best effort), `--reply-timeout` (15 seconds by default) from publishing to the reply, 1 second of
cleanup.

## Discovery And Versioning

`schema` lists `admin.order-info` as JSON-capable with generated input and output schemas; `docs/schemas/discovery-v1.json` is
regenerated and `docs/schemas/admin-order-info-v1.json` is added and drift-checked. Adding a command with its own codes is a
compatible change within schema version 1. The open-enum fields are plain strings in the output schema, so a value added by a later
node is not a contract change.
