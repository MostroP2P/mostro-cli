# Feature Specification: A Reply Deadline The Caller Can Set

**Feature**: `016-reply-timeout`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux against the scripted node (see `tests/TEST_SUMMARY.md`). Progress is tracked only in
[tasks.md](tasks.md)

**Input**: A harness run against a regtest mostrod (v0.20.2, SQLite in rollback-journal mode) hit daemon-side `database is locked`
contention: the node answered a `take-sell` after 24 s, but the fixed 15 s reply budget (`msg::ExchangeLimits::reply`) had already made it
`uncertain`. The mobile app has no such budget.

## Requirements

- **FR-001**: A global option `--reply-timeout SECONDS`, accepted at any command depth like `--output`, 5 to 300, default 15 (nothing changes
  without it). Out of range or not a number is `invalid_arguments` (exit 2) before anything runs.
- **FR-002**: It sets the reply deadline of every command that waits for a node reply: `order new`, `order take-sell`, `order take-buy`,
  `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order info`, `dispute open`, `order rate`, `bond add-invoice`
  (the registry's request and write commands). The connect, probe and cleanup budgets do not change. The public reads (`order list`,
  `message list`, `order sync`) keep their own limits and ignore it; the legacy commands ignore it.
- **FR-003**: Discovery lists it: `data.global_options` (and the top-level `global_options` of the human `schema` output) holds one entry per
  global option with `name`, `type`, `description`, `default` when there is one, `enum` when there is one, and `applies_to`, the canonical
  names it takes effect on; generated from the Clap grammar and the registry, drift-tested in `docs/schemas/discovery-v1.json`.

## Success Criteria

- **SC-001**: Against a silent scripted node, each of the eleven commands stops at 5 s with `--reply-timeout 5` (`reply_timeout`: `uncertain`,
  or `error` for `order info`), well before 15 s.
- **SC-002**: A node that answers after 16 s: `--reply-timeout 20` gets the answer (`ok`), the default does not (`uncertain`).
- **SC-003**: `4`, `301`, `0`, text and an empty value are `invalid_arguments` with the command named, before or after the command words.
