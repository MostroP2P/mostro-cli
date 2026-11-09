# Data Model: Order Watch

The contract types of unit 1 (`outcome::watch`, `outcome::watch::cursor` and `TradeRole` in `outcome::messages`). Later units build the
transport and the operation on them; `order watch` emits them in human mode from unit 5 and serializes them with `--output json` from
unit 6.

## Input (`WatchInput`)

| Field (private, with getters) | Meaning |
| --- | --- |
| `order` | The order's uuid |
| `since_minutes()` | The window, 1 to 10080 minutes, default 30; `None` with `--after` |
| `after()` | The cursor given with `--after` |
| `timeout_secs()` | 1 to 86400 seconds; without `--timeout`, 30 in JSON and none in human mode |
| `start_cursor(now)` | The cursor given, or the window before `now` with no ids |

`InvalidWatch` names the option that failed (`Since`, `After`, `SinceWithAfter`, `Timeout`) and never its value.

## `Cursor`

| Part | Rule |
| --- | --- |
| Wire form | `<since>` or `<since>:<id>,<id>...`; pattern `^(0\|[1-9][0-9]{0,19})(:[0-9a-f]{64}(,[0-9a-f]{64})*)?$`, at most 13,020 characters |
| `since` | Decimal Unix seconds, no sign, no leading zeros, within 64 bits |
| `ids` | 1 to 200 distinct lowercase 64-hex record ids, written sorted |
| `Observed` | A record a call accepted: `mail(id, time)`, a node message, whose re-publication keeps its time; `chat(id, outer, inner)`, a chat message by its inner id, another envelope of which can be dated up to 60 s after `inner` |
| `advance(observed, now)` | Time: the newest observed outer time, clamped to `now`, minus 60 s, never lower than before, held at the newest envelope of any chat message whose envelopes all fall before it while another one could still be dated at or after it; above 200 records at or after it, the time rises to the smallest second that leaves at most 200, never past `now` minus 60 s (nor past the old time when later); ids: those observed at or after the time, the newest 200 when more still qualify; nothing observed keeps the cursor |

## `TradeRole`

`Buyer` or `Seller`: `of_order(kind, made)` is `Seller` for a sell order made or a buy order taken, `Buyer` otherwise.

## `order.watch` data (`WatchData`)

| Field | Type | Meaning |
| --- | --- | --- |
| `order_id` | uuid | The order |
| `counterparty_pubkey` | hex64 or null | The counterparty's trade key; null while the chat is not followed |
| `messages` | array of `MessageRecord` | The node's new messages about the order, oldest first, as `message list` reports them |
| `chat` | array of `ChatRecord` | The new chat messages, oldest first, each once by its `message_id` (spec 019) |
| `ended` | bool | One of the messages ended the trade (D9) |
| `truncated` | bool | Older records may be missing: a relay's replay reached its limit (D4) |
| `cursor` | string | Pass it back verbatim with `--after` (D7) |

`meta.snapshot` is always null and every stage is `not_applicable`.

## Outcomes

| Status | Exit | When |
| --- | --- | --- |
| `ok` | 0 | The first new records after the replays, or empty at the deadline (nothing observed; relays may have been down) |
| `error` | 2 | `invalid_arguments` |
| `error` | 1 | Any other code below |

Errors: `invalid_arguments`, `configuration_missing`, `configuration_invalid`, `identity_missing`, `identity_unreadable`,
`store_unreadable`, `order_unknown`, `relay_connection_failed`, `relay_read_failed`, `read_timeout`, `read_limit_exceeded`,
`internal_error`. Warnings: `invalid_event_timestamp`, `cleanup_incomplete`. No `uncertain`.
