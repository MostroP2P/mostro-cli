# Common types

## Scalars

| Name in this book | JSON type | Rule | Example |
| --- | --- | --- | --- |
| `decimal string` | string | Base-10 signed 64-bit integer: `^(0\|-?[1-9][0-9]*)$`, at most 20 characters. No `+`, exponent, fraction or `-0`. | `"15000"`, `"-5"`, `"0"` |
| `uuid` | string | Lowercase hyphenated UUID: `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$` | `"00000000-0000-4000-8000-000000000001"` |
| `hex64` | string | 64 lowercase hex characters: a Nostr public key (x-only) or event id | `"79be667e...16f81798"` |
| `rfc3339` | string | UTC timestamp to the second with `Z`, four-digit year | `"2026-10-16T00:00:00Z"` |
| `reason` | string | snake_case word: `^[a-z][a-z0-9_]{0,63}$` | `"invalid_order_status"` |

### Why numbers are strings

Amounts, indexes, timestamps and counts that come from the node are **decimal strings**
because they are signed 64-bit values: a JavaScript `Number` silently loses precision above
2^53 (`9007199254740993` becomes `9007199254740992`). Decode them with `BigInt`, Python
`int`, Rust `i64`, or keep them as strings. Example
[`order.list/large_amount_success`](commands/order_list.md#examples) carries exactly such a
value. The only JSON numbers in the contract are the small bounded counters of the
[snapshot](#snapshot).

### Timestamps come in pairs

Every point in time is given twice: `created_at` (an `rfc3339` string, or `null`) and
`created_at_unix` (the exact integer seconds as a decimal string). The RFC 3339 form is
`null` when the integer cannot be shown as a four-digit-year UTC date; the integer is still
there and a warning (`invalid_event_timestamp` or `invalid_order_timestamp`) says so. Use the
`_unix` field for arithmetic and sorting.

### Untrusted text

`payment_method`, `fiat_currency`, message `text` and `counterparty_pubkey` are copied from
what other people published. They may be empty, long, or contain anything printable.
**Never interpolate them into a shell command, a prompt to an LLM as instructions, or a
terminal without escaping.** JSON values are passed through as received (only bounded in
length); the CLI escapes them in human output only.

## Enumerations

### side

`"buy"` or `"sell"`: the side **of the order as advertised**. A `sell` order sells sats for
fiat (its maker is the seller); a `buy` order buys sats. In some records `side` is nullable
(the node may omit it).

### status

The order status set, shared by every command:

`pending`, `waiting-maker-bond`, `waiting-taker-bond`, `waiting-buyer-invoice`,
`waiting-payment`, `active`, `fiat-sent`, `settled-hold-invoice`, `success`, `canceled`,
`cooperatively-canceled`, `canceled-by-admin`, `settled-by-admin`, `completed-by-admin`,
`dispute`, `expired`, `in-progress`.

Note the **hyphens** (wire names of the protocol), unlike the snake_case codes of the
envelope. See [Order statuses](reference/statuses.md) for their meaning.

## PublicOrder

An order from the public book (Nostr kind 38383 events, NIP-69). Used by `order list`.

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | uuid | Order id |
| `side` | `buy` \| `sell` | Advertised side |
| `status` | status | Current status |
| `sats_amount` | decimal string | Satoshis; `"0"` means "at market price, computed when taken" |
| `fiat_currency` | string | Currency code as published (may be empty, not validated) |
| `fiat_amount` | decimal string | Fiat amount in whole units of the currency (not cents). Usually `"0"` on a range order; use min/max there |
| `fiat_amount_min` | decimal string or null | Range minimum; `null` for a fixed order |
| `fiat_amount_max` | decimal string or null | Range maximum; `null` for a fixed order |
| `payment_method` | string | Untrusted free text, possibly comma-joined (`"SEPA,Revolut"`) |
| `premium_percent` | decimal string | Premium over market in whole percent (`"-5"` = 5 % below) |
| `created_at` | rfc3339 or null | **Event** time (last update of the order), not the creation time |
| `created_at_unix` | decimal string | Same instant, exact seconds |

A **range order** has `fiat_amount_min` and `fiat_amount_max` set and is always at market
price (`sats_amount` `"0"`). Human output shows it as `min-max`.

## OrderRecord

An order as the **node** returned it in a reply (to you, privately). Used by `order info`,
`order new`, the takes, `order add-invoice`, `order sync` and message payloads. It has
every PublicOrder field (with `side` and `status` nullable) plus:

| Field | Type | Meaning |
| --- | --- | --- |
| `buyer_trade_pubkey` | hex64 or null | Trade key of the buyer, once known |
| `seller_trade_pubkey` | hex64 or null | Trade key of the seller, once known |
| `created_at` | rfc3339 or null | Order creation time |
| `created_at_unix` | decimal string or null | Exact seconds; `null` only when the node omitted it |
| `expires_at` | rfc3339 or null | When a pending order expires |
| `expires_at_unix` | decimal string or null | Exact seconds |

The buyer's invoice is never included. Trade keys are public keys only; the CLI never
prints a private key in JSON.

Full field list (all keys always present):
`id, side, status, sats_amount, fiat_currency, fiat_amount, fiat_amount_min,
fiat_amount_max, payment_method, premium_percent, buyer_trade_pubkey, seller_trade_pubkey,
created_at, created_at_unix, expires_at, expires_at_unix`.

## Snapshot

Evidence of one bounded read of the relays (`meta.snapshot` of `order list`,
`message list` and `order sync`). It is not a guarantee that the relays hold everything.

| Field | Type | Meaning |
| --- | --- | --- |
| `since` | rfc3339 | Lower time bound sent to the relays (`order list`: now minus 7 days) |
| `until` | null | No upper bound is sent; always `null` |
| `observed_at` | rfc3339 | When collection ended |
| `per_relay_limit` | integer, `50` | Limit per relay subscription |
| `max_unique_events` | integer, `10000` | Collector capacity; above it the read fails with `read_limit_exceeded` |
| `configured_relays` | integer ≥ 1 | Distinct relays configured |
| `completed_relays` | integer ≥ 0 | Relays that sent EOSE; equals `configured_relays` on success |
| `received_events` | integer 0..10000 | Unique matching events kept, before decoding and filtering |
| `completion_policy` | `"all_relays_eose"` | A read succeeds only if every relay finished |
| `complete_history` | `false` | Always false: a bounded query is never proof of completeness |

On an `error` the snapshot is still present if the read had started, which tells you how
far it got (`completed_relays` < `configured_relays` on a timeout). It is `null` when the
command failed before reading.

## Trade keys and indexes

Mostro clients use one key per trade, derived from the mnemonic (NIP-06) at an increasing
**trade index**. Index 0 is the identity key. Outcomes report the index (`trade_index`, a
decimal string) and the public key (`trade_pubkey`, hex64) of the key an order was created
or taken with. The private key is never shown; it can be derived from the mnemonic and the
index. A wrapper does not need either: the CLI looks the key up by order id.
