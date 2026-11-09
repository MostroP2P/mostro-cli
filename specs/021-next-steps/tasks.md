# Tasks: Next Steps Of Message Records

**Mode**: lean, as [spec 017](../017-advertised-pow/tasks.md) (same gates). No push, no public node. No Git mutation without
authorization.

## Unit 1: the payment_failed payload and the order id in Next lines

- [x] **1. Failing tests.** Unit: the moved `next_step.rs` table and the payment-failed counts, the `payment_failed` summary, the
  human Next lines with the order id and the retry line. Process: the human flow expects the order ids; a mailbox of two failed payouts
  (2 and 0 retries). Integration: the spec 021 example against the checked-in schema and the owned constructors, and invalid variants.
  Eight failed on the previous code (one example test passed with no example yet).
- [x] **2. Next step, payload and human lines.** As in [plan.md](plan.md).
- [x] **3. Fixtures and docs.** Three schema fixtures regenerated; the versioning rule and `message list` in `docs/commands.md`,
  README, architecture, spec 001 and 006 pointers, evidence.

## Unit 2: fiat_sent and rate

- [x] **4. Failing tests.** Unit: the `next_step.rs` table, the `ends_trade` and `rate_follows` rules, the fiat-sent and rate Next
  lines. Process: the flow's `hold-invoice-payment-accepted` now `fiat_sent`; both ends of a trade then `rate`; the human Next lines;
  `order watch` ends after the `rate` (human, JSON, and the grace window with no `rate`); the human watch prints the rate line.
  Integration: the `messages_of_the_trade_steps` example. Ten failed against stubs that added the two values without behaviour.
- [x] **5. Next steps, lines and the watch end.** As in [plan.md](plan.md).
- [x] **6. Fixtures and docs.** Three schema fixtures regenerated; `docs/commands.md` (`message list`, `order release`, `order watch`,
  versioning), architecture, README walkthroughs, AGENTS, spec 020 D9 pointer, evidence.

## Unit 3: release

- [x] **7. Failing tests.** Unit: the `next_step.rs` table over the three sides and both request-id cases, the side derivation, the
  `Roles` lookup, the release Next line. Process: the four kind and role rows (seller release, buyer none, the buyer's copy with a
  request id none); no row, NULL `is_mine` and another key; a legacy row; the human release line; a store without its orders table is
  `store_unreadable` with no relay asked; a seller's `order watch` reports `release`, a buyer's does not. Integration: the
  `the_seller_is_asked_to_release` example. Nine behaviour tests failed against stubs.
- [x] **8. Roles, the release step and the line.** As in [plan.md](plan.md).
- [x] **9. Fixtures and docs.** Three schema fixtures regenerated; `docs/commands.md` (`message list`, versioning), `docs/database.md`
  (`is_mine`), architecture, README seller walkthrough, AGENTS, evidence.
- [ ] Regtest probe with two identities: the request ids of both `fiat-sent-ok` copies. Not run: this unit used no node.
