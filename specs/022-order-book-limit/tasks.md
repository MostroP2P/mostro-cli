# Tasks: A Complete Order Book In `order list`

**Mode**: lean, as [spec 017](../017-advertised-pow/tasks.md) (same gates). No push, no public node. No Git mutation without
authorization. Each PR stays under 1,500 production lines ([plan.md](plan.md), Pull Request Budget).

## Unit 1 / PR 1: status filter at the relay and the 300 limit

- [x] **1. Failing tests.** Unit: `order_book_filter` (`#s` spelling for three statuses, limit 300, same `since`). Process: the
  Story 1 book lists 33 (14 buy, 19 sell); `--status canceled` sends `#s: ["canceled"]`; a relay ignoring `#s` does not resurface an
  order whose latest state differs; `per_relay_limit` is 300 for `order.list`. Schema: `order.list` pins 300, `message.list` keeps
  50. Failed before the change: the Story 1 book listed 23 of 33, and the REQ had no `#s`. The relay ignoring `#s` already
  passed (kept as a regression). `listing_selects_latest_matching_states_across_relays` now asserts the accepted risk.
- [x] **2. Filter and limit.** `ORDER_BOOK_LIMIT`, `order_book_filter`, the shared `since` helper, the call in `list.rs`, the
  per-command `per_relay_limit` schema rule.
- [x] **3. Fixtures.** Regenerate `order-list-v1.json` and `discovery-v1.json`.
- [x] **4. Gates and budget.** fmt, clippy, test, build, schema drift; production-line count under 1,500.

## Unit 2 / PR 2: `data.truncated`

- [x] **5. Failing tests.** Unit: per-relay counts in `Collection` (at the limit, below, union above, duplicates). Process: 301
  pending orders give 300 and `truncated: true`; the Story 1 book gives `false`; the human stderr warning only when truncated.
  Schema: `truncated` required.
- [x] **6. Count and flag.** `RelayProgress.events`, `Collection::reached_limit`, `Fetched::truncated`, `ListData.truncated`,
  `select_orders` wiring.
- [x] **7. Human warning.** The FR-005 line in `print_list`.
- [x] **8. Fixtures and docs.** Schemas regenerated; `docs/commands.md` (listing semantics, accepted risk, Versioning exception),
  `docs/architecture.md`, spec 001 pointers, `AGENTS.md`, `tests/TEST_SUMMARY.md`.
- [x] **9. Gates and budget.** As task 4.

## After both PRs

- [ ] Follow-up in the MCP repository and the book (the 50-event wording).
- [ ] Optional SC-005 read-only probe against the production relays.
