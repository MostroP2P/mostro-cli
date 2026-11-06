# Feature Specification: Order Expiry In Seconds

**Feature**: `012-order-expires-in`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux against the scripted node (see `tests/TEST_SUMMARY.md`). Progress is tracked only in
[tasks.md](tasks.md)

**Input**: An end-to-end harness needs orders that expire quickly (seconds or minutes) to test expiry flows; `--expiration-days` has a one-day
granularity. Amends [order new](../003-order-new-json/spec.md) (its documents are marked where they change).

## Clarifications

### Session 2026-11-05

- Q: What does the node do with `expires_at`? -> A: mostrod (`src/util.rs`, `get_expiration_date`) uses the request's `expires_at` as given and
  only lowers it to `now + max_expiration_days`; without it the node applies its default. A short value is legal.
- Q: Range? -> A: `--expires-in SECONDS`, 1 to 31536000 (365 days, the `--expiration-days` maximum); the order carries
  `expires_at = now + SECONDS`.
- Q: Both options? -> A: `--expires-in` conflicts with `--expiration-days`: `invalid_arguments` (exit 2) before any effect, in both modes.

## Requirements

- **FR-001**: `order new ... --expires-in SECONDS`; outside 1..=31536000 or together with `--expiration-days` is `invalid_arguments` before
  any effect.
- **FR-002**: The request's order carries `expires_at = now + SECONDS`; `--expiration-days` and the default are unchanged.
- **FR-003**: Discovery lists the input property `expires-in` on `order.new` (generated from the grammar); the human preview shows it.

## Success Criteria

- **SC-001**: Against the scripted node, `--expires-in 60` sends `expires_at` within a second of `now + 60`.
- **SC-002**: `0`, `31536001`, a non-number and the combination with `--expiration-days` exit 2 with `invalid_arguments`, nothing sent, the store
  byte-identical.
- **SC-003**: `docs/schemas/discovery-v1.json` has `expires-in` in the `order.new` input schema.
