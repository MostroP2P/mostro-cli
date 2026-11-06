# Feature Specification: Open A Dispute For Humans And Agents

**Feature**: `013-dispute-open`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux against the scripted node (see `tests/TEST_SUMMARY.md`); not probed on a real node. Progress is
tracked only in [tasks.md](tasks.md)

**Input**: `dispute open --order UUID [--yes]`, canonical `dispute.open`, replaces the legacy `dispute -o|--orderid` with no alias. Same shape
as the [trade steps](../007-trade-steps-json/spec.md): the order's stored key signs, one request, one correlated reply.

## Findings (mostrod v0.20.2 source, `src/app/dispute.rs`, `src/app.rs`)

- The request is `dispute` on the order, no payload, no trade index. Only an `active` or `fiat-sent` order can be disputed
  (`cant-do` `not_allowed_by_status`), only by its buyer or seller (`cant-do` `invalid_pubkey`).
- On success the node sets the order to `dispute` and sends the initiator `dispute-initiated-by-you` with `Payload::Dispute(dispute_id, None)`,
  echoing the request id; the other party gets `dispute-initiated-by-peer`.
- A second dispute on the same order, and a few internal checks, end in an internal error: **the node sends nothing**. The CLI reports silence
  as `uncertain` (`reply_timeout`) and never retries.

## Requirements

- **FR-001**: `dispute open --order UUID [--yes]`; the legacy `dispute`, `-o` and `--orderid` (documented as `--order-id`) are removed with no alias. JSON requires `--yes`;
  human mode previews and asks.
- **FR-002**: The order's stored trade key signs; an order not in the store is `order_unknown` and nothing is sent. No trade index is spent.
- **FR-003**: The reply is selected by the request id; allowed actions `dispute-initiated-by-you` and `cant-do`; the order it names and a
  `dispute` payload are checked. A `cant-do` counts as a refusal only when it names the order (mostrod echoes it); one for another order or
  none is an unusable reply, `uncertain` (`reply_malformed`) (amended 2026-11-05 after review; the same for `order rate`).
- **FR-004**: `ok`: `data` = `order_id`, `dispute_id`, `trade_pubkey`, `node_pubkey`; the local status becomes `dispute` (a failed update is
  the warning `local_status_stale`). `error`: `node_refused` with its reason, or a local failure before sending. `uncertain`: no reply, an
  unusable reply, a relay failure after publishing.
- **FR-005**: Output schema `docs/schemas/dispute-open-v1.json`, regenerated discovery, examples, drift tests.

## Success Criteria

- **SC-001**: Against the scripted node, an active order is disputed: one request, `ok`, the dispute id of the reply, local status `dispute`.
- **SC-002**: A pending order is refused (`not_allowed_by_status`), a second dispute is `uncertain` (`reply_timeout`), an unknown order sends
  nothing.
- **SC-003**: `dispute` is gone; JSON without `--yes` does nothing.
