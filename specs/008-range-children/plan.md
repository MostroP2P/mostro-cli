# Implementation Plan: Range Order Children

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-02 | **Status**: Implemented (slices 1-6) with Linux evidence against a scripted node and mailbox; behavior against a real node is unverified.

## Summary

Three pieces: an exact maker marker in the store; the next trade key and index in `fiat-sent` and `release` for a maker's range order (the
`Stages::Step` vocabulary already allows the index record on `uncertain`); and `order sync`, a write command that adopts the node's `new-order`
messages for orders the store does not know. Reuse: `msg::inbox::read_inbox` and `Store::trade_keys` from `message list`, `Store::save_order` and
the index policy from `order new`, `cli/write.rs`, the envelope and the schema generator.

## Design

1. **Maker marker.** `save_order` takes whether the order is the user's (`order new` true, `order take-*` false); `range_has_room` requires it.
   Adjust the callers and tests; the scripted-daemon tests that seed rows state the marker.
2. **Key and index.** `commands/order/step.rs`: when `kind`, the stored order's kind and `is_mine` call for it (new `Store::range_step_needs_key`),
   take `store.next_trade()` before publishing, add the payload, and after the exchange spend the index like `Sent::spend_index` (amended
   2026-11-05 by spec 008, slice 7: `store.reserve_trade()` records the index before publishing; a refusal or a request that never left gives it
   back). `StepData` carries
   `next_trade_index` (already in the schema) and the new `next_step` value `sync_child_order`.
3. **Adoption.** `commands/order/sync.rs`: resolve keys with `Store::trade_keys`, read the inbox, keep `new-order` messages whose order is unknown,
   save each with `Store::save_order` (a variant that takes the key by index). `outcome/sync.rs`, `cli/sync.rs`, `parse_human/sync.rs`.
4. **Discovery and docs.** Schema and fixture for `order.sync`, regenerated discovery, examples, README and command reference, the amended spec 007
   texts.

## Risks

- **Spending an index for nothing**: a legacy row of a taker is treated as the user's; the node ignores the key and the index is spent. Accepted and
  documented; no migration.
- **Adopting a stranger's order**: only `new-order` messages authored by the node and addressed to one of the user's derived keys are read.
- **Duplicate rows**: `save_order` upserts on the id; `order sync` skips known ids, so an existing row is never replaced.
- **Buy range order timing**: the child exists only after the peer releases; `order sync` before that finds nothing and says so.

## Complexity Tracking

| Addition | Why it is needed | Simpler option rejected |
| --- | --- | --- |
| New command `order sync` | one place gives both range cases and any missed child a row, without waiting inside a step | waiting inside `release` (helps one case only) |
| `is_mine` made exact | the key must go to makers only; takers are mislabeled today | keep the legacy `Some(_)` test |
