# Data Model: Order Info

**Status**: Proposed v1 additions; not implemented. No persistent model or migration: the
command only reads the existing local identity. The envelope, the stage enums, the order
encodings and the decimal-string rules are those of the
[order-list data model](../001-order-list-json/data-model.md) and are not repeated.

## Input

| Field | Rule |
| --- | --- |
| `--order` (one or more) | UUID, any case accepted, canonicalised to lowercase hyphenated. A repeated id is kept once, at its first position. No count cap. At least one is required |
| root options | `--mostro-pubkey` and `--relays` (CLI over `MOSTRO_PUBKEY` and `RELAYS`, no fallback from an invalid CLI value), `--pow` / `POW` (0-255, default 0), `--transport` / `TRANSPORT` (only `nip44`). `--secret` is accepted and has no effect on this account-scoped request |

Invalid or missing input is `invalid_arguments` (exit 2) before any configuration, identity or network
access; invalid configuration values are `configuration_invalid` (exit 1).

## Command identity

Canonical name `order.info` (approved in contract v1). CLI path `order info`. It replaces the
human-only `ordersinfo` entry, whose canonical name it already had. Capability: JSON supported with
the *request* stage vocabulary below.

## Success data: `OrderInfoData`

| Field | Wire type | Meaning |
| --- | --- | --- |
| `node_pubkey` | 64 lowercase hex | The configured node |
| `orders` | array of `OrderRecord` | Returned orders in the order of the request, each at most once, only requested ids |
| `not_returned` | array of lowercase UUID strings | Requested distinct ids absent from the reply, in request order. The node gives no reason |

Invariant: every requested distinct id appears in exactly one of `orders[].id` and `not_returned`.

`OrderRecord` (explicit fields; no core or database object is serialized):

| Field | Wire type | Notes |
| --- | --- | --- |
| `id` | lowercase UUID | |
| `side` | `buy`, `sell` or null | The order's advertised side; null if the node omits it |
| `status` | one of the canonical statuses or null | Same set as `order.list` |
| `sats_amount` | decimal string | Signed 64-bit; `0` is market priced |
| `fiat_currency` | string | As received, possibly empty |
| `fiat_amount` | decimal string | |
| `fiat_amount_min`, `fiat_amount_max` | decimal string or null | |
| `payment_method` | string | Untrusted text, as received |
| `premium_percent` | decimal string | |
| `buyer_trade_pubkey`, `seller_trade_pubkey` | 64 lowercase hex or null | Public keys only. A value that does not parse as a public key becomes null |
| `created_at`, `expires_at` | RFC3339 `Z` string or null | Null when absent or not representable |
| `created_at_unix`, `expires_at_unix` | decimal string or null | The exact integers; null only when absent |

The buyer invoice is never included (the daemon clears it, and the field is not part of the contract).

## Errors

All exit 1 unless noted; `data` is null on every error; messages are fixed prose.

| Code | When | Stages (publication / daemon / persistence) |
| --- | --- | --- |
| `invalid_arguments` (2) | missing, malformed or unknown arguments, removed spelling | not_attempted / unknown / not_applicable |
| `configuration_missing`, `configuration_invalid` | node key, relays, `--pow`, `--transport` | same |
| `identity_missing` | no data directory, `mcli.db` or user row | same |
| `identity_unreadable` | the store exists but cannot be read or holds no valid mnemonic | same |
| `relay_connection_failed` | a configured relay could not be connected in time | same |
| `protocol_unsupported` | the node advertises protocol 1 | same |
| `pow_required` | advertised work is above the configured work, found before publishing | not_attempted / unknown / not_applicable |
| `pow_required` | same, found after a timeout | published / unknown / not_applicable |
| `request_rejected` | no relay confirmed accepting the request event | not_attempted or unknown / unknown / not_applicable (see below) |
| `reply_timeout` | published, no matching reply before the deadline | published / unknown / not_applicable |
| `relay_read_failed` | a relay failed or closed the subscription while waiting | published / unknown / not_applicable |
| `node_refused` | `CantDo` other than `not_found`; `error.reason` is the node's reason in snake case, for example `too_many_requests` or `invalid_parameters` | published / rejected / not_applicable |
| `reply_malformed` | a matching reply with an unusable action or payload | published / unknown / not_applicable |
| `internal_error` | unexpected local failure | stages at the point of failure |

`request_rejected` means no relay confirmed accepting the request event. Its publication stage is
`not_attempted` when every relay answered with an explicit rejection, and `unknown` when at least one
relay failed ambiguously (for example a timeout or a dropped connection after the send), because that relay
may still have the event. A read is safe to repeat, so no outcome here is `uncertain`.

`error.reason` appears only on `node_refused`: `{ "code": "node_refused", "message": "...",
"reason": "too_many_requests" }`. Messages never include the reason's free text.

## Success stages

`published / accepted / not_applicable`, also for the `not_found` success. `meta.snapshot` is null.

## Warnings

| Code | When |
| --- | --- |
| `orders_not_returned` | `not_returned` is not empty (including the `not_found` case) |
| `invalid_order_timestamp` | a returned `created_at` or `expires_at` cannot be shown as a UTC date |

## Exchange lifecycle

validated (arguments) -> configured (node key, relays, work, transport) -> identity read (read-only) ->
connected (10 s) -> node info probed (3 s, best effort) -> pre-checks (protocol, work) -> subscription
armed -> request published (exactly once) -> awaiting a correlated reply (15 s) -> replied | timed out |
failed -> cleanup (1 s) -> rendered -> written. Each relay accepts or rejects the request event; the
outcome is published if at least one accepted. Unrelated, duplicate, own-echo, wrong-author and
wrong-request messages are ignored while waiting. Cancellation releases resources but cannot
guarantee a final document.

## Discovery

`order.info` is described like the other machine commands: input schema from Clap (the repeatable `--order`,
required, with the UUID format), output schema from these types, `required_configuration`
`MOSTRO_PUBKEY` and `RELAYS`. It lists the codes above in addition to the frozen pilot set in its own `$defs`.
