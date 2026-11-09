# Feature Specification: Next Steps Of Message Records

**Feature**: `021-next-steps`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-09

**Status**: Partly implemented. Unit 1 (FR-004, FR-005 for the existing steps and the retry line, FR-006, FR-009) is implemented and
verified on Linux against the scripted node only (see `tests/TEST_SUMMARY.md`). FR-001, FR-002, FR-003, FR-007 and FR-008 are proposed
(units 2 and 3). Progress is tracked only in [tasks.md](tasks.md).

**Input**: issue #243. A `message list` record names a `next_step` for four actions only, so an agent following a trade must know the
rest of the protocol to send the fiat, release or rate. A failed payout arrived as `{"kind":"other","variant":"payment_failed"}`, without
its retry count. The human "👉 Next:" lines printed `<id>` instead of the order id the record carries.

Node behaviour, read from the mostrod v0.20.2 source only (not observed on a node; unit 3 adds a regtest probe):

| Action | Recipient | Request id | Payload |
| --- | --- | --- | --- |
| `hold-invoice-payment-accepted` | buyer only | the triggering request's id, or none | `Order` |
| `fiat-sent-ok` | both | seller copy: none; buyer copy: the buyer's `fiat-sent` request id | `Peer` |
| `hold-invoice-payment-settled`, then `rate` | seller | settled: the release request id; rate: none | none |
| `purchase-completed`, then `rate` | buyer | rate may carry one | none |
| `payment-failed` | buyer, once per buyer invoice (first failed payout to it; again after a replacement invoice resets the count), never updated | | `PaymentFailed { payment_attempts, payment_retries_interval }` |

`payment_attempts` is the retries left: the node's setting minus one (default setting 3, so 2), and 0 with a setting of 1 or 0. At 0
the node neither retries nor sends `add-invoice`; it accepts a new invoice in `settled-hold-invoice`. Retries are skipped while LND is
unsynced, so the interval is approximate.

## Requirements

- **FR-001** (unit 2): `hold-invoice-payment-accepted` gives `next_step` `fiat_sent`.
- **FR-002** (unit 3): the seller's copy of `fiat-sent-ok` gives `release`: only when the local row of the order says the user is the
  seller (its kind and whether the user made it), the message went to that row's trade key, and it carries no request id.
- **FR-003** (unit 2): `rate` gives `rate`.
- **FR-004**: `payment-failed` with a `PaymentFailed` payload is the payload kind `payment_failed` with `payment_attempts` and
  `payment_retries_interval` as decimal strings, like every number in `message.list` data. Its `next_step` is `null` while the node
  retries and `add_invoice` at `payment_attempts` `"0"`.
- **FR-005**: each human "👉 Next:" line names its command with the record's order id filled in (`<id>` only when the record names no
  order). `payment_failed` prints "🔁 Payment failed: the node will retry the payment up to N more times, every M seconds" (singular for
  1), or at 0 "the node will not retry the payment" followed by the add-invoice Next line.
- **FR-006**: `schema_version` stays `"1"`. A record's `next_step` and its payload's `kind` are open enums within v1: a later executable
  may add values, and may move a payload out of `other` into a kind of its own. A consumer treats an unknown `next_step` as `null` and a
  payload of an unknown `kind` as opaque (it reads only `kind`); the `variant` of `other` is not a stable identifier. The generated
  schemas stay closed and describe the executable that generated them. Written in the versioning rule of `docs/commands.md`.
- **FR-007** (unit 3): reading the order rows fails as `store_unreadable` before any relay is read.
- **FR-008** (units 2 and 3): every command that reports message records applies these rules: `message list` and `order watch`
  (which then ends after the `rate` that follows the end of the trade).
- **FR-009**: the docs state that a `next_step` is what its message asked when the node sent it: check the order's status with
  `order info` before acting, and act on each `event_id` once.

## Success Criteria

- **SC-001**: against the scripted node's mailbox, a `payment-failed` with 2 retries is `payment_failed` `"2"`/`"60"` with `next_step`
  `null`, and one with 0 has `next_step` `add_invoice`; the JSON conforms to `docs/schemas/message-list-v1.json`.
- **SC-002**: the human output of the existing flow names `order add-invoice --order <the order id>` and `bond add-invoice --order <the
  order id>`, and never `<id>` when every record names its order.
- **SC-003** (units 2 and 3): the trade-step values on the scripted mailbox; a regtest probe with two identities records the request
  ids of both `fiat-sent-ok` copies.

## Out Of Scope

The `next_step` of the write results; the other `<id>` placeholders in human output; exposing the user's side in JSON; a mostro-mcp
`order.rate` tool and its tool descriptions (a separate repository).
