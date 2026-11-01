# Research: Add Invoice

Read 2026-11-01 in the workspace `mostro` checkout (v0.20.1 plus three commits): `src/app/add_invoice.rs`, `src/app/bond/payout.rs`,
`src/util.rs`, `mostro-core/src/message.rs`, and the legacy `src/cli/add_invoice.rs`, `src/cli/add_bond_invoice.rs` here. Not checked against a running
daemon (regtest pins v0.19.3).

## `add-invoice` on the node

`add_invoice_action`: the order is read by id, only the buyer (`event.sender` equals the stored buyer trade key) may answer (`invalid_peer`), the
invoice is validated against the order amount and fee (`invalid_invoice`), then by status:
- `waiting-buyer-invoice`: with a stored preimage the order goes active, the seller gets `buyer-took-order` and the buyer gets
  `hold-invoice-payment-accepted` with the order; otherwise a hold invoice is created, the seller gets the pay request and the **buyer gets
  `waiting-seller-to-pay` with no payload**, echoing the request id.
- `settled-hold-invoice`: a replacement invoice is stored and the buyer gets `invoice-updated` (no payload), request id echoed.
- any other status: `cant-do(not_allowed_by_status)`.
The request carries `Payload::PaymentRequest(None, invoice, None)`, no trade index.

## `add-bond-invoice` on the node

`add_bond_invoice_action` finds a recoverable bond of that order for the sender (`not_allowed_by_status` when there is none, the share is zero or the
claim window passed), validates the invoice amount against the counterparty share (`invalid_invoice`, no fee subtracted), stores it and sends
`bond-invoice-accepted` with an order payload through `enqueue_payout_ack`, which sets **no request id**. Payment follows on the scheduler; a later
`bond-payout-completed` is out of scope. Older nodes send nothing on success (the legacy command's "timeout is success" reading).

## Consequences

- Correlation must accept the order id (and author and addressee) when the reply has no request id; `msg::exchange` needs that option.
- `bond add-invoice` cannot distinguish an old node's silence from a lost ack: `uncertain` is the honest status.
- Neither command spends a trade index; the stored trade key signs. `order add-invoice` needs the store writable (status update), `bond
  add-invoice` does not.
- The legacy `bond` path accepted only a BOLT-11 and checked it locally; that check is kept as argument validation.
