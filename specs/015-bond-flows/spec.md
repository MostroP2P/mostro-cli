# Feature Specification: Bond Flows In The Migrated Commands

**Feature**: `015-bond-flows`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Verified on Linux against the scripted node and mailbox (see `tests/TEST_SUMMARY.md`); not probed on a bonded real node in this
spec. Progress is tracked only in [tasks.md](tasks.md)

**Input**: An end-to-end harness runs bonded nodes. It needs (a) `order cancel` to work for a taker whose take waits on its bond and for a maker
whose order waits on its bond, and (b) `message list --order` to expose the bond messages. Verification of existing commands; a defect found
is fixed here.

## Findings (mostrod v0.20.2 source)

- A bonded take answers `pay-bond-invoice` with the order (status `pending` on the wire; the node parks it at `waiting-taker-bond`). A bonded
  `new-order` answers `pay-bond-invoice` (status `pending` on the wire; the node keeps it unpublished at `waiting-maker-bond`). `order take-*`
  and `order new` save the row with that order and the trade key (`next_step` `pay_bond_invoice`).
- `cancel` at `waiting-taker-bond` from the taker releases its bond, answers `canceled` (request id echoed) and puts the order back to
  `pending` for other takers; from the maker it cancels the order. `cancel` at `waiting-maker-bond` from the maker closes the unpublished order
  and answers `canceled` (request id echoed); if the bond locked meanwhile it is refused with `not_allowed_by_status`; from anyone else
  `is_not_your_order`.
- Bond messages: `add-bond-invoice` (`Payload::BondPayoutRequest`, the order's `amount` is the share), `bond-invoice-accepted` and
  `bond-payout-completed` (`Payload::Order`, amount = share, no request id), `bond-slashed` (`Payload::Order`, amount = the slashed bond,
  status absent).

## Requirements

- **FR-001**: After a bonded `order take-*` or `order new` (`pay_bond_invoice`), `order cancel --order <id>` finds the row and its key and maps
  `canceled` to `data.result` `canceled`, local status `canceled`; a refusal is `node_refused` with its reason.
- **FR-002**: `message list --order <id>` returns `add-bond-invoice` with `next_step` `add_bond_invoice` and the `bond_payout_request` payload
  (the share in `payload.order.sats_amount`, `slashed_at`), and `bond-invoice-accepted`, `bond-payout-completed`, `bond-slashed` with their
  action names and an `order` payload.

## Success Criteria

- **SC-001**: Both cancels pass against a scripted node that applies the source's rules, through the real binary, starting from the rows the
  bonded take and order created.
- **SC-002**: The four bond messages come back from the scripted mailbox with the documented fields.
