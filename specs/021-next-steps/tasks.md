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

## Unit 2: fiat_sent and rate (proposed)

- [ ] Failing tests, `NextStep::FiatSent` and `NextStep::Rate`, Next lines, the `order watch` end after `rate`, fixtures and docs.

## Unit 3: release (proposed)

- [ ] Failing tests, `is_mine` in `StoredOrder`, `Roles`, `NextStep::Release`, fixtures and docs.
- [ ] Regtest probe with two identities: the request ids of both `fiat-sent-ok` copies.
