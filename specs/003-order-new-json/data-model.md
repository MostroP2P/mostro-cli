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
| `invoice` | optional text, bounded (4096 bytes), only on a buy order |
| `expiration_days` | integer 0..=365, default 0 (the node's default) |
| `yes` | boolean; required in JSON mode |

Local validation only catches what is cheap and certain; the daemon remains the authority (currency list, quotes, invoice
amounts). A rejection by the daemon is `node_refused` with its reason.

## Output (`OrderNewData`)

`order`: the node's order as `OrderRecord` (id, kind, status, amounts, currency, payment method, premium, timestamps);
`trade_index`: integer; `trade_pubkey`: 64 hex characters (public). In an `uncertain` outcome after an accepted reply the same
`data` is present (it is what the node answered) and `error.code` is `order_not_saved`; after an unknown outcome `data` is null.

## Statuses and stages

| Case | status | publication | daemon_acceptance | persistence |
| --- | --- | --- | --- | --- |
| accepted and saved | ok | published | accepted | saved |
| refused by the node (`CantDo`) | error | published | rejected | not_applicable |
| accepted, save failed | uncertain | published | accepted | failed |
| no reply before the deadline, unusable reply | uncertain | published | unknown | unknown |
| bonded node asks for a bond | error | published | accepted | not_applicable |
| no relay accepted the event (rejected) | error | not_attempted | unknown | not_applicable |
| relay answers ambiguous | uncertain | unknown | unknown | unknown |
| local failure (arguments, identity, configuration, work, protocol, declined) | error | not_attempted | unknown | not_applicable |

(The stage names `saved`, `failed` and `unknown` for `persistence` are checked against the pilot enum in slice 2.)

New codes: `confirmation_required` (JSON without `--yes`, exit 2 as `invalid_arguments`? see the contract), `order_declined`
(human `n`), `bond_required`, `order_not_saved`, `store_unwritable` (the store cannot be opened for writing before publishing).
