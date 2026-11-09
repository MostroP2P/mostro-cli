# Implementation Plan: Next Steps Of Message Records

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-09 | **Status**: Units 1, 2 and 3 implemented with Linux evidence against the scripted node;
the regtest probe was not run.

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

## Unit 2 (implemented)

- `outcome/messages/next_step.rs`: `NextStep::FiatSent` (`hold-invoice-payment-accepted`) and `NextStep::Rate` (`rate`), from the
  action alone; no store read and no signature change.
- `parse_human/message.rs`: their Next lines (`order fiat-sent --order ID`, `order rate --order ID --rating <1-5>`).
- `outcome/watch.rs`: `ends_trade` also ends at `rate`; `rate_follows` names `purchase-completed` and `hold-invoice-payment-settled`.
- `commands/order/watch.rs`: `with_rate` keeps reading after a batch that holds one of those without a `rate`, until the `rate`, the
  grace window (`WatchLimits::rate_grace`, 10 s) or the deadline; `Watched::ended_by` names the end action rather than the `rate`.
  In an update, a `rate` sorts after the other records of its second, so it follows the end even when both share a `created_at`.
- Fixtures: the same three regenerated; the `messages_of_the_trade_steps` example.

## Unit 3

`NextStep::Release` with `NextStep::of(&MessageKind, Option<TradeRole>)`; `TradeRole` and `TradeRole::of_order` move from
`messages.rs` to `messages/next_step.rs` (`StoredOrder.is_mine` already came with spec 020). A `Roles` lookup in
`commands/message/roles.rs` (`from_rows`, `load`, `of`) is loaded once before any relay read by `message list` and `order watch`;
`order watch` takes its own side from it too, replacing its private row lookup. `MessageRecord::from_message` gains a seventh
parameter, `role: Option<TradeRole>` (within clippy's limit, so no struct); `commands::message::list::record` passes
`roles.of(order, &mail.key)`. Fixtures: the same three regenerated; the `the_seller_is_asked_to_release` example.
