# Feature Specification: Rate The Counterparty For Humans And Agents

**Feature**: `014-order-rate`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux against the scripted node (see `tests/TEST_SUMMARY.md`); not probed on a real node. Progress is
tracked only in [tasks.md](tasks.md)

**Input**: `order rate --order UUID --rating 1-5 [--yes]`, canonical `order.rate`, replaces the legacy `rate` with no alias. Same shape as
[dispute open](../013-dispute-open/spec.md).

## Findings (mostrod v0.20.2 source, `src/app/rate_user.rs`)

- The request is `rate-user` with `Payload::RatingUser(r)`. The order must be `success`, or `settled-hold-invoice` when the seller rates
  (`cant-do` `invalid_order_status`). The reply is `rate-received` with `Payload::RatingUser(r)` to the rater, echoing the request id.
- A side that already rated, a counterparty in full-privacy mode, and a sender who is neither party get **no answer**; the CLI reports silence
  as `uncertain` (`reply_timeout`).
- The rating writes nothing locally (the legacy command did not either).

## Requirements

- **FR-001**: `order rate --order UUID --rating N [--yes]`; `N` outside 1..=5 is `invalid_arguments` before any effect; `rate`, `-o`,
  `--orderid` (documented as `--order-id`), `-r` are removed with no alias. JSON requires `--yes`.
- **FR-002**: The order's stored trade key signs; `order_unknown` sends nothing; no trade index.
- **FR-003**: Correlated by request id; allowed `rate-received`, `cant-do`; the reply's rating must equal the one sent.
- **FR-004**: `ok`: `data` = `order_id`, `rating` (JSON integer 1-5), `trade_pubkey`, `node_pubkey`; persistence `not_attempted` (no local
  write). Errors and `uncertain` as `dispute open`.
- **FR-005**: Output schema `docs/schemas/order-rate-v1.json`, discovery, examples, drift tests.

## Success Criteria

- **SC-001**: Against the scripted node, a successful order is rated: `ok` with the rating.
- **SC-002**: An active order is refused (`invalid_order_status`); a second rating is `uncertain`; `--rating 0` and `6` exit 2, nothing sent.
- **SC-003**: `rate` is gone; JSON without `--yes` does nothing; the store is byte-identical after every run.
