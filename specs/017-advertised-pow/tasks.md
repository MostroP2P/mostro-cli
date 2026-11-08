# Tasks: Mine The Proof Of Work The Node Advertises

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md) (same gates). No push, no public node; the regtest probe uses a
separate mostrod on a local stack. No Git mutation without authorization.

- [x] **1. Failing tests.** Unit: lane resolution, unmet work, `work_to_mine`, `with_work` on the envelope, the schema rule,
  the human line (all failed to compile). Process: the existing "more work expected" cases (they asserted `pow_required` for
  9 and 12 bits, which is now mined) moved above the ceiling, plus mined-work assertions (`Received.work`) for each command
  family, the lane and floor case, and the timeout explained by first-contact work.
- [x] **2. Node info, exchange, outcome, schema, commands, CLI line.** As in [plan.md](plan.md).
- [x] **3. Fixtures and docs.** Twelve schema fixtures regenerated (the pilot ones unchanged), `docs/pow_error_handling.md`
  §5.2 and §10, command reference, README, architecture, evidence.
- [x] **4. Regtest probe.** A separate mostrod with `pow` > 0 on the local stack accepts `order new` with `POW` unset.
