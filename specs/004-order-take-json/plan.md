# Implementation Plan: Order Take

**Spec**: [spec.md](spec.md) | **Date**: 2026-10-30 | **Status**: Proposed design; no implementation, no runtime evidence.

## Summary

`order take-sell` and `order take-buy` (replacing `takesell` and `takebuy`) are one operation parameterised by the side: one signed take published
through `msg::exchange` with a fresh trade key, the reply selected by correlation and mapped to a `next_step`, the row and the trade index saved in
one transaction, `uncertain` when the node may hold the take. Almost everything is reuse of `order new`: identity, store, exchange, envelope,
stages, prompt, schema rules. Evidence and decisions: [research.md](research.md).

## Technical Context

Rust 2021, MSRV 1.94, no new dependency, test side on the loopback relay, the scripted fake daemon (extended with take replies) and the process
harness. Files stay at or below 400 production lines; imports at the top; tests before code; no live node, public relay or regtest without
authorization.

## Constitution Check

| Principle | Compliance |
| --- | --- |
| Imports, size, reuse | new `commands/order/take.rs`, `outcome/take.rs`, `cli/order_take.rs`, `parse_human/order_take.rs`; the prepare/prompt/print logic shared with `order new` moves to one module first (`cli/write.rs`) so neither dispatch grows past 400 lines; reuse exchange, identity, `Store`, envelope, schema generator, `OrderRecord` |
| Boundaries | wrapper returns a typed outcome; renderers pure; the only prompt lives in `cli/`; transport only in `msg`; storage writes only through `Store`, after a reply |
| Evidence | legacy `takesell`/`takebuy` characterized first (payload bytes, save path); scripted daemon from the source; regtest probe separate |
| Safety | a write command: the `order new` confirmation rule, one publish, no retry, the index never reused after publication, no key material, invoice or hold invoice in diagnostics, sentinel tests |

## Design

1. **Shared write dispatch.** Extract from `cli/order_new.rs` the steps both commands need (configuration, identity, store checks, trade, prompt
   with a preview supplied by the command, emit/print, verbose line) into `cli/write.rs`; `order new` keeps its tests unchanged.
2. **Input and data.** `outcome/take.rs`: `OrderTakeInput::validate` (UUID, amount, invoice), `payload(side)` (the legacy bytes), `OrderTakeData` and
   `from_reply`; `NextStep`; the minimal `SmallOrder` for `waiting-seller-to-pay`.
3. **Operation.** `commands/order/take.rs`: `order_take(side, input, config, identity, trade, store, options, limits)` using `exchange` with allowed
   actions `add-invoice`, `waiting-seller-to-pay`, `pay-invoice`, `pay-bond-invoice`, `cant-do`; mapping per the data model; the same index policy as
   `order new` (`spend_index` helper shared).
4. **Dispatch and grammar.** `OrderCommands::TakeSell(TakeSellArgs)` and `TakeBuy(TakeBuyArgs)`; two registry entries (`JsonSupport::Write`) replacing the
   legacy ones; `cli/order_take.rs`; `parse_human/order_take.rs`; legacy `Commands::TakeSell/TakeBuy`, `cli/take_order.rs` and `save_taken_sell_order`
   removed.
5. **Discovery.** Command-scoped schemas for each canonical name (same rules, different `D`), `order-take-sell-v1.json` and `order-take-buy-v1.json`
   fixtures, regenerated discovery; other fixtures do not change.

## Risks

- **Daemon version**: v0.20.1 source vs regtest v0.19.3; the probe decides claims.
- **Minimal row**: a take-sell with an invoice leaves empty currency and payment method locally until a refresh that does not exist yet; later commands
  must tolerate it (they did for the legacy fallback).
- **Duplicate takes**: an `uncertain` run may have taken the order; documented; the node refuses a second take.
- **Hold invoice**: returned in the result; callers must not log it. Documented.
- **MCP**: `take_order` stays broken until migrated.
- **Cashu escrow**: not modeled; reported as `reply_malformed`.

## Complexity Tracking

| Addition | Why it is needed | Simpler option rejected |
| --- | --- | --- |
| `cli/write.rs` extraction | two write commands would duplicate the 200-line prepare/prompt/print flow and push files past the limit | copy `order_new.rs` |
| One operation for two sides | the exchange, mapping and save are identical but for the payload and the reply set | two near-identical files |
