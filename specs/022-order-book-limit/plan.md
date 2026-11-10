# Implementation Plan: A Complete Order Book In `order list`

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-10 | **Status**: Proposed.

## Pull Request Budget

Each pull request stays under **1,500 lines of production code**: added plus removed lines in `src/`, excluding `#[cfg(test)]`
blocks and test-only files (`tests.rs`, `tests/` modules). Tests, fixtures, generated schemas, specs and docs do not count.
Measure before opening the PR:

```sh
git diff --numstat origin/main...HEAD -- src/ ':!src/**/tests.rs' ':!src/**/tests/**' \
  | awk '{n += $1 + $2} END {print n}'
```

then subtract the lines changed inside inline `#[cfg(test)] mod tests { ... }` blocks. The change is small, so the split serves
review, not size:

| PR | Unit | Production estimate | Ceiling |
| --- | --- | --- | --- |
| PR 1 | Unit 1: relay-side status filter and the 300 limit | ~60 lines | 1,500 |
| PR 2 | Unit 2: `data.truncated`, the human warning, schemas, docs | ~150 lines | 1,500 |

If a PR would exceed its ceiling (for example, a file split forced by the 400-line rule), move whole tasks to a later PR; never split
one task across PRs. PR 2 depends on PR 1 (it reads `ORDER_BOOK_LIMIT`), so it is opened after PR 1 merges or stacked on it.

## Unit 1 (PR 1): status filter and limit

Changes:

- `src/msg/orders.rs` (82 lines):
  - `pub(crate) const ORDER_BOOK_LIMIT: usize = 300;` beside `PER_RELAY_LIMIT` (unchanged, 50).
  - `pub(crate) fn order_book_filter(author: PublicKey, status: Status, now: DateTime<Utc>) -> Result<Filter, ErrorCode>`:
    kind `NOSTR_ORDER_EVENT_KIND`, `#z: order`, `since` seven days before `now`, `.limit(ORDER_BOOK_LIMIT)` and
    `.custom_tag(SingleLetterTag::LOWERCASE_S, status.to_string())` (`Status`'s `Display` is the kebab-case tag value; a unit test
    pins it). A private `seven_days_before(now)` holds the `since` computation shared with `seven_day_filter` (constitution III).
  - `seven_day_filter` keeps its signature and its 50 for `src/util/events.rs` (FR-006).
- `src/msg/mod.rs`: re-export `order_book_filter` and `ORDER_BOOK_LIMIT`.
- `src/commands/order/list.rs`: call `order_book_filter(config.mostro_pubkey(), filters.status(), now)` instead of
  `seven_day_filter(...)`. `reduce_orders` keeps its status filter (SC-003).
- `src/outcome/schema.rs`: `per_relay_limit` is no longer one constant for every command. The `order.list` snapshot rule (the
  snapshot value it passes to `meta_rule`) pins `per_relay_limit` to 300; the other commands keep 50. Choose between a
  `per_relay_limit(limit)` builder used by each command's rule and an `enum [50, 300]` base narrowed per command, whichever keeps
  `schema.rs` under 400 production lines.
- Generated schemas: `docs/schemas/order-list-v1.json` and `docs/schemas/discovery-v1.json`, regenerated with the `REGENERATE`
  command from `tests/README.md` (no hand edits).

Tests first (constitution V); each must fail on the current code before the change:

- `src/msg/orders.rs` unit tests: `order_book_filter` carries `#s` with `pending`, `in-progress` and `canceled` for the three
  statuses; its limit is 300; its `since` equals `seven_day_filter`'s for the same `now`.
- `tests/order_list_process.rs`:
  - the Story 1 book on a loopback `LocalRelay` (33 pending, 14 buy and 19 sell, plus 27 newer orders in other statuses):
    `order list --output json` lists 33 (SC-001); `--side buy` lists 14 and `--side sell` lists 19.
  - `--status canceled` lists only canceled orders, and the scripted relay recorded `#s: ["canceled"]` in the REQ (Story 3).
  - a scripted relay that ignores `#s` and returns an order's newer `in-progress` event beside its older `pending` one: the order is
    not listed (SC-003).
  - the existing `json["per_relay_limit"] == 50` assertion (around line 776) becomes 300 for `order.list`.
- Schema tests: `order.list` validates with 300 and rejects 50; `message.list` keeps 50.

## Unit 2 (PR 2): truncation flag

Changes:

- `src/msg/receive_msg.rs` (253 lines):
  - `RelayProgress` gains `events: usize`, incremented in `observe` for each event of the subscription that matches the filter,
    before the uniqueness check in `keep`, so another relay's copy does not lower a relay's own count.
  - `Collection::reached_limit(&self) -> bool`: any relay with `events >= filter.limit`.
  - `Fetched` gains `truncated: bool` and `pub fn truncated(&self) -> bool`. `FetchFailure` does not need it (error outcomes carry
    no data).
- `src/outcome/order.rs` (261 lines): `ListData` gains `pub(crate) truncated: bool`, documented for the schema ("a relay returned its
  limit of matching events, so older orders in this status may be missing"); `ListData::from_selected` takes `truncated`.
- `src/commands/order/list.rs`: pass `fetched.truncated()` through `select_orders`.
- `print_list` (human renderer of `order list`): when `data.truncated`, write the FR-005 line to stderr after the table; stdout is
  unchanged.
- `src/msg/inbox.rs`, `src/msg/chat.rs`: they compute their own `truncated`. Switch them to `Fetched::truncated()` only where the rule
  is identical; `inbox.rs` counts unique events, so it stays as it is. This spec changes no behaviour of theirs.
- Generated schemas: `order-list-v1.json`, `discovery-v1.json`.
- Docs: `docs/commands.md` listing semantics (status at the relay, 300, `truncated`, the accepted risk) and Versioning (the D4
  exception); `docs/architecture.md` (the per-relay count of `fetch_msg`); spec 001 `contracts/cli.md` and `data-model.md` get a
  one-line pointer to this spec (history is not rewritten); `AGENTS.md` status line; `tests/TEST_SUMMARY.md` evidence.

Tests first:

- `src/msg/receive_msg/tests.rs`: a relay delivering exactly the limit sets `truncated`; one below does not; two relays each below the
  limit whose union is above it do not; a duplicate across relays counts for each relay.
- `tests/order_list_process.rs`: 301 pending orders on one relay give 300 listed and `truncated: true` (SC-002); the Story 1 book
  gives `truncated: false`; human mode prints the warning on stderr only when truncated, and stdout is byte-identical otherwise.
- Schema tests: `data.truncated` is required in `order.list` success; an example without it fails validation.

SC-002 seeds 301 signed events. Generate them once per test with fixed keys and timestamps, as the existing book fixtures do, to keep
the test fast.

## Gates (each PR)

`cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked`,
`cargo build --locked`, the schema drift check and the production-line count above. Report the CI Windows run when it completes.

## Follow-ups Outside This Repository

- The MCP server's `list_orders` description and the book page that quote "50 events per relay" (the text the agent quoted).
- The optional SC-005 read-only probe against the production relays, after PR 2.
