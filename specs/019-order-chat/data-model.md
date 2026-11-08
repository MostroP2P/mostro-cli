# Data Model: Order Chat

Proposed for units 2-4; only the lookup types of unit 1 exist in the code.

## The lookup (unit 1, `commands::order::counterparty`)

| Type | Fields (private, with getters) | Meaning |
| --- | --- | --- |
| `Counterparty` | `key: PublicKey`, `warnings: Warnings` | The counterparty's trade key and the warnings of the read of the node's mail (none when the row held the key) |
| `LookupFailure` | `code: ErrorCode`, `warnings: Warnings` | `identity_missing`, `store_unreadable`, the read's own code, or `counterparty_unknown`; the read's warnings |

## `order.chat` data (`ChatData`)

| Field | Type | Meaning |
| --- | --- | --- |
| `order_id` | uuid | The order |
| `counterparty_pubkey` | hex64 | The counterparty's trade key |
| `messages` | array of `ChatRecord` | Oldest first by (`created_at`, `message_id`); at most 200; never the message sent on this run |
| `truncated` | bool | A relay or the cap may have left out older messages |
| `sent` | `ChatRecord` or null | The message sent on this run, non-null exactly when publication is `published` or `unknown` |

## `ChatRecord`

| Field | Type | Meaning |
| --- | --- | --- |
| `message_id` | hex64 | The inner event id |
| `from` | `me` or `counterparty` | Who wrote it: `me` when the inner signer is our trade key |
| `created_at` | RFC 3339 or null | The inner event time; null (with `invalid_event_timestamp`) when it cannot be shown |
| `created_at_unix` | decimal string | The exact integer |
| `text` | string | Untrusted, at most 4096 bytes, cut on a character boundary |

`meta.snapshot` is always null; `persistence` and `daemon_acceptance` are always `not_applicable` (D3, D4).

## Outcomes

| Status | Exit | Stages (publication / acceptance / persistence) | When |
| --- | --- | --- | --- |
| `ok` | 0 | `not_applicable` x3 | read |
| `ok` | 0 | `published` / na / na | sent |
| `error` | 2 | na x3 | `invalid_arguments`: bad arguments, or `--message` without `--yes` in JSON |
| `error` | 1 | na x3 | configuration, identity or store codes, `order_unknown`, `counterparty_unknown`, lookup or read failures |
| `error` | 1 | `not_attempted` / na / na | send step: `request_rejected` (every relay refused), `relay_connection_failed` or `internal_error` |
| `uncertain` | 1 | `unknown` / na / na | `request_rejected` with data (`messages` and `sent`) |

Errors: `invalid_arguments`, `configuration_missing`, `configuration_invalid`, `identity_missing`, `identity_unreadable`,
`store_unreadable`, `order_unknown`, `counterparty_unknown`, `relay_connection_failed`, `relay_read_failed`, `read_timeout`,
`read_limit_exceeded`, `request_rejected`, `internal_error`. Warnings: `invalid_event_timestamp`, `cleanup_incomplete`.
