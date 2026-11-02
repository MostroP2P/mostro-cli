# Error codes

`error.code` is the stable value to branch on. `error.message` is the fixed sentence shown
here; do not match on it. Each command's schema lists the subset it can emit (see the
command pages and the [matrix](#which-command-emits-which-code) below).

## Catalogue

| Code | Exit | Message | Meaning and what to do |
| --- | --- | --- | --- |
| `invalid_arguments` | 2 | The command arguments are invalid. | A missing, unknown or malformed option; an out-of-range value; a removed spelling; a write in JSON mode without `--yes`. Nothing was done. Fix the call. The offending value is never echoed. |
| `output_not_supported` | 2 | This command does not support JSON output. | A human-only command was asked for JSON. Nothing ran. |
| `configuration_missing` | 1 | Required node configuration is missing. | `MOSTRO_PUBKEY` or `RELAYS` (or their options) unset or empty. |
| `configuration_invalid` | 1 | The node configuration is invalid. | Unparsable node key or relay URL, bad `--pow` or `--transport`. |
| `relay_connection_failed` | 1 | A configured relay could not be connected. | Within 10 s. Check the network and relay list; safe to retry. |
| `relay_read_failed` | 1 | A configured relay failed during the public order read. | A relay closed or dropped the subscription. For a read: retry. **After publishing a write, this comes as `uncertain`.** The message mentions "public order read" for every command. |
| `read_timeout` | 1 | The public order read did not complete before its deadline. | Not every relay sent EOSE within 15 s. No partial data is returned. Retry, maybe with fewer or faster relays. |
| `read_limit_exceeded` | 1 | The public order read exceeded its event limit. | More than 10 000 matching events. Narrow the read (`--since`). |
| `internal_error` | 1 | An internal error occurred. | A bug or an output that could not be written. Report it. |
| `identity_missing` | 1 | No local identity exists. | No data directory, `mcli.db` or user row. Create one (see [Getting started](getting_started.md#create-an-identity)) or point `MCLI_DIR` at the right place. |
| `identity_unreadable` | 1 | The local identity store cannot be read. | The database exists but cannot be opened or holds no valid mnemonic. |
| `protocol_unsupported` | 1 | The node speaks a protocol version this program does not support. | The node advertises protocol v1 (gift wrap). Nothing sent. |
| `pow_required` | 1 | The node requires more proof of work than is configured. | Raise `--pow` / `POW` to the node's value. Usually found before sending (`error`); when found only after a timeout, it is `uncertain`. |
| `request_rejected` | 1 | No relay confirmed accepting the request. | `publication: not_attempted` ⇒ every relay refused the event, provably nothing sent (`error`). `publication: unknown` ⇒ a relay failed ambiguously; for a write this is `uncertain`. |
| `reply_timeout` | 1 | The node did not answer before the deadline. | Published, no correlated reply within 15 s. For `order info` an `error` (repeatable); for writes `uncertain`. |
| `node_refused` | 1 | The node refused the request. | The node answered `cant-do`. `error.reason` carries its reason when it is a snake_case word. The node did not act. |
| `reply_malformed` | 1 | The reply from the node could not be used. | A correlated reply with an unexpected action or payload. For writes `uncertain`. |
| `order_declined` | 1 | The order was not confirmed, so nothing was sent. | The user answered no at the **human** prompt. A JSON caller cannot get it (JSON requires `--yes`). |
| `order_not_saved` | 1 | The node created the order but it could not be saved locally. | Always `uncertain`. The node accepted; `data` holds what it answered; the local row is missing. For `order sync`: some rows were saved, one failed. |
| `store_unwritable` | 1 | The local store cannot be written, so nothing was sent. | The trade index cannot be reserved or the row cannot be updated. Fix permissions/disk; nothing happened on the network. |
| `order_unknown` | 1 | No local order has this id, so nothing was sent. | The order is not in the local `orders` table (wrong id, another data directory, a range child not yet synced, a restored mnemonic without the database). Try `order sync`. |
| `store_unreadable` | 1 | The local store cannot be read. | The orders table cannot be read. |

## Refusal reasons

With `node_refused`, `error.reason` is the node's `cant-do` reason. It comes from the Mostro
daemon, so the set can grow; treat unknown values generically. Reasons seen in the CLI's
tests and probes:

| reason | Typical cause |
| --- | --- |
| `too_many_requests` | Rate limited; wait and retry later |
| `invalid_parameters` | The request was not acceptable |
| `invalid_amount` | Amount outside the node's limits, or not in the range |
| `invalid_order_status` | The order is not in a status that allows this step (already taken, canceled...) |
| `invalid_peer` | You are not a party of this order / not the right party for this step |
| `invalid_pubkey` | The trade key does not match the order |
| `invalid_invoice` | The invoice was rejected (amount, expiry, network) |
| `order_already_canceled` | Nothing to cancel |
| `not_found` | Only `order info`: turned into a **success** with every id in `not_returned` |

The [protocol book](https://mostro.network/protocol/) has the daemon's full list of
`cant-do` reasons.

## Which command emits which code

`E` = as `error`, `U` = as `uncertain`, `E/U` = either, depending on the stages.

| Code | list | info | new | take-* | add-invoice | bond | fiat-sent / release / cancel | sync | message list | schema |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `invalid_arguments` | E | E | E | E | E | E | E | E | E | E |
| `configuration_missing` | E | E | E | E | E | E | E | E | E | |
| `configuration_invalid` | E | E | E | E | E | E | E | E | E | |
| `relay_connection_failed` | E | E | E | E | E | E | E | E | E | |
| `relay_read_failed` | E | E | E/U | E/U | E/U | E/U | E/U | E | E | |
| `read_timeout` | E | | | | | | | E | E | |
| `read_limit_exceeded` | E | | | | | | | E | E | |
| `internal_error` | E | E | E | E | E | E | E | E | E | E |
| `identity_missing` | | E | E | E | E | E | E | E | E | |
| `identity_unreadable` | | E | E | E | E | E | E | E | E | |
| `protocol_unsupported` | | E | E | E | E | E | E | | | |
| `pow_required` | | E | E/U | E/U | E/U | E/U | E/U | | | |
| `request_rejected` | | E | E/U | E/U | E/U | E/U | E/U | | | |
| `reply_timeout` | | E | U | U | U | U | U | | | |
| `node_refused` | | E | E | E | E | E | E | | | |
| `reply_malformed` | | E | U | U | U | U | U | | | |
| `order_declined` | | | E | E | E | E | E | | | |
| `order_not_saved` | | | U | U | | | | U | | |
| `store_unwritable` | | | E | E | E | | E | E | | |
| `order_unknown` | | | | | E | E | E | | E | |
| `store_unreadable` | | | | | | | | E | E | |

`output_not_supported` is emitted only for [human-only commands](commands/legacy.md).
