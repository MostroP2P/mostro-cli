# Tasks: dispute open

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md) (same gates and evidence entry).

- [x] **1. Scripted node and failing tests.** `step_daemon` disputes with the source's rules; process tests in `tests/dispute_rate_process.rs`.
- [x] **2. Shared request, outcome, command.** `commands/order/held.rs`, `outcome/dispute.rs`, `commands/order/dispute.rs`, `cli/held.rs`,
  `parse_human/held.rs`, grammar, registry, the legacy command removed.
- [x] **3. Fixtures, examples, docs.**
