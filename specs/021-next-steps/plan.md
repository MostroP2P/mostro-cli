# Implementation Plan: Next Steps Of Message Records

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-09 | **Status**: Unit 1 implemented with Linux evidence against the scripted node;
units 2 and 3 proposed.

## Unit 1 (implemented)

- `outcome/messages/next_step.rs` (new): `NextStep`, moved out of `messages.rs` for file size, and `NextStep::of(&MessageKind)`, which
  reads the whole message: the four existing actions, and `payment-failed` -> `add_invoice` only with a `PaymentFailed` payload whose
  `payment_attempts` is 0. `messages.rs` re-exports it, so `outcome::messages::NextStep` is unchanged.
- `outcome/messages/payload.rs`: `MessagePayload::PaymentFailed { payment_attempts, payment_retries_interval }` (decimal strings),
  before `Other`; the descriptions of `MessagePayload` (an unknown kind is opaque) and `Other` (the variant is not stable).
- `outcome/messages.rs`: the description of `MessageRecord.next_step` (what the message asked when sent; unknown values read as null).
- `parse_human/message.rs`: `next_step(step, order)` fills the record's order id; `retry_line(attempts, interval)` for
  `payment_failed`. `order watch` reuses `block`, so its human output gets both.
- Fixtures: `message-list-v1.json`, `discovery-v1.json` and `order-watch-v1.json` regenerated with the `REGENERATE` command.
- `docs/commands.md`: the versioning exception, the stale-hint rule and the failed payout under `message list`.

## Unit 2 (proposed)

`NextStep::FiatSent` (`hold-invoice-payment-accepted`) and `NextStep::Rate` (`rate`), their Next lines (`order fiat-sent --order ID`,
`order rate --order ID --rating <1-5>`), and the `order watch` end moved after the `rate` that follows `purchase-completed` or
`hold-invoice-payment-settled` (with a bounded grace window when no `rate` comes).

## Unit 3 (proposed)

`NextStep::Release` with `NextStep::of(&MessageKind, Option<TradeRole>)`; `StoredOrder.is_mine`; a `Roles` lookup in
`commands/message/roles.rs` loaded once before any relay read by `message list` and `order watch`; the role passed to every
`MessageRecord::from_message` call (group the addressing data in a struct if clippy's argument limit is reached).
