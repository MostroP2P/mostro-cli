# Data Model: Order Take

Reuses the order-new pieces: `OrderRecord`, the envelope with `uncertain`, `ExecutionMeta::write`, `OutcomeRules::write`, `error.reason`, the
codes `order_declined`, `bond_required`, `order_not_saved`, `store_unwritable`, and `Store`. No new error or warning code.

## Input (`OrderTakeInput`)

| Field | Rule |
| --- | --- |
| `order` | UUID, canonicalised to lowercase |
| `amount` | optional integer > 0: the fiat amount for a range order; the node checks the range |
| `invoice` | take-sell only; optional; a BOLT-11 invoice or a Lightning address, at most 4096 bytes, no whitespace or control characters |
| `yes` | boolean; required in JSON mode |

`take-buy` has no `--invoice`. The payload bytes follow the legacy command (characterized in slice 1): take-buy sends `Amount(n)` only when an
amount is given; take-sell sends the invoice (with the amount inside the same payment request) or `Amount(n)`, and `Amount(0)` when neither is given.

## Output (`OrderTakeData`)

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the configured node |
| `order_id` | UUID of the order taken |
| `side` | the side of the order taken: `sell` (take-sell) or `buy` (take-buy) |
| `next_step` | `add_invoice` (the taker must send a buyer invoice), `wait_for_seller_payment` (the invoice was accepted), `pay_hold_invoice` (the taker, as seller, must pay `payment_request`) |
| `order` | the node's order as `OrderRecord`, or null when the reply carried none (`wait_for_seller_payment`) |
| `payment_request` | the hold invoice, only with `pay_hold_invoice`, else null |
| `trade_index` | decimal string |
| `trade_pubkey` | 64 lowercase hex; the private key is derivable from the mnemonic and the index and is never shown |

## Statuses and stages

As for `order new` (`persistence`: `applied` for the order row and index, or the index alone; `failed`; `not_attempted` when no write is
called for). The data of an `order_not_saved` outcome is the same `OrderTakeData`.

| Case | status | code | publication | daemon_acceptance | persistence |
| --- | --- | --- | --- | --- | --- |
| accepted and saved | ok | none | published | accepted | applied |
| local failure (arguments, identity, configuration, work, protocol, declined, store) | error | its code | not_attempted | unknown | not_attempted |
| no relay accepted the event (proven) | error | `request_rejected` | not_attempted | unknown | not_attempted |
| refused by the node (`CantDo`) | error | `node_refused` + reason | published | rejected | not_attempted |
| bond asked first | error | `bond_required` | published | accepted | applied or failed |
| accepted, row not saved | uncertain | `order_not_saved` (data present) | published | accepted | failed |
| no reply before the deadline | uncertain | `reply_timeout` | published | unknown | applied or failed |
| unusable reply (wrong action or payload, Cashu escrow) | uncertain | `reply_malformed` | published | unknown | applied or failed |
| relays dropped the wait after publishing | uncertain | `relay_read_failed` | published | unknown | applied or failed |
| relays answered ambiguously | uncertain | `request_rejected` | unknown | unknown | applied or failed |

A JSON run without `--yes` is `invalid_arguments` (exit 2).

## The local row

From the reply order when there is one (status as the node sent it); otherwise (`waiting-seller-to-pay`) a minimal `SmallOrder`: the order id,
kind `sell`, status `waiting-payment`, the fiat amount if given (else 0), empty currency and payment method, saved with the trade key and the
trade index in the single transaction of `Store::save_order`.
