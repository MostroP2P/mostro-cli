# Data Model: Order New

Reuses the order-info pieces: `OrderRecord` (explicit fields, no buyer invoice), the envelope, `ExecutionMeta::request`,
`error.reason`.

## Input (`OrderNewInput`)

| Field | Rule |
| --- | --- |
| `side` | `buy` or `sell` (the order's side) |
| `currency` | 3 to 6 ASCII letters or digits, uppercased |
| `fiat_amount` | integer > 0; with `fiat_amount_max` it is the range minimum |
| `fiat_amount_max` | optional integer > `fiat_amount`; makes a range order (then `sats` must be 0) |
| `payment_method` | non-empty text, bounded (200 bytes), no control characters |
| `sats` | integer >= 0, default 0 (market price) |
| `premium` | integer in -100..=100, default 0 |
| `invoice` | optional text, bounded (4096 bytes), no whitespace or control characters; which side may carry one is the node's rule |
| `expiration_days` | integer 0..=365, default 0 (the node's default) |
| `yes` | boolean; required in JSON mode |

Local validation only catches what is cheap and certain; the daemon remains the authority (currency list, quotes, invoice
amounts). A rejection by the daemon is `node_refused` with its reason.

## Output (`OrderNewData`)

`order`: the node's order as `OrderRecord` (id, kind, status, amounts, currency, payment method, premium, timestamps);
`trade_index`: integer; `trade_pubkey`: 64 hex characters (public). In an `uncertain` outcome after an accepted reply the same
`data` is present (it is what the node answered) and `error.code` is `order_not_saved`; after an unknown outcome `data` is null.

## Statuses and stages

`persistence` says whether the local write the outcome calls for happened: `applied` (the order row and index, or only the index
when there is no order to save), `failed`, or `not_attempted` when no write is called for (a refusal, a local failure). The trade
index is advanced whenever the request was published and not provably rejected.

| Case | status | code | publication | daemon_acceptance | persistence |
| --- | --- | --- | --- | --- | --- |
| accepted and saved | ok | none | published | accepted | applied |
| local failure (arguments, identity, configuration, work, protocol, declined, store) | error | its code | not_attempted | unknown | not_attempted |
| no relay accepted the event (proven) | error | `request_rejected` | not_attempted | unknown | not_attempted |
| refused by the node (`CantDo`) | error | `node_refused` + reason | published | rejected | not_attempted |
| bonded node asks for a bond | error | `bond_required` | published | accepted | applied or failed |
| accepted, order row not saved | uncertain | `order_not_saved` (data present) | published | accepted | failed |
| no reply before the deadline | uncertain | `reply_timeout` | published | unknown | applied or failed |
| unusable reply | uncertain | `reply_malformed` | published | unknown | applied or failed |
| relays dropped the wait after publishing | uncertain | `relay_read_failed` | published | unknown | applied or failed |
| relays answered ambiguously | uncertain | `request_rejected` | unknown | unknown | applied or failed |

New codes (all exit 1, scoped to this command's schema): `order_declined`, `bond_required`, `order_not_saved`, `store_unwritable`.
A JSON run without `--yes` is `invalid_arguments` (exit 2). Implemented in slice 3: `Envelope::uncertain`, `ExecutionMeta::write`,
`OutcomeRules::write`; the registry entry and the generated fixtures follow in slice 4 with the command, as for order info.
