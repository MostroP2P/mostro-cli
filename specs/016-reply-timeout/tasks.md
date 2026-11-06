# Tasks: Reply Deadline

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md) (same gates).

- [x] **1. Failing tests.** `tests/reply_timeout_process.rs` (4: every waiting command stops at 5 s; a late answer taken with 20 s and missed by
  the default; out-of-range values; a public read ignores it), `the_global_options_are_listed_with_what_they_apply_to`.
- [x] **2. Option, limits, discovery.** `cli/args.rs`, `cli/write.rs::exchange_limits`, the dispatch sites, `global_options`.
- [x] **3. Docs.** Command reference, README, discovery fixture, evidence.
