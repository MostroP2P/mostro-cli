# Implementation Plan: Trade Steps

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-02 | **Status**: Draft

## Summary

Three commands that are one operation parameterised by the step: sign one request with the order's stored trade key, `exchange` with the
step's allowed actions (order-id correlation as in `order add-invoice`), map the reply to a typed result, update the local status. Almost
everything is reuse of spec 005 (`Store::trade_for_order`, `set_status`, `order_unknown`, `cli/write.rs`, envelope, schema rules); the new
parts are the three reply sets and the `range_not_continued` warning (range continuation is spec 008).

## Design

1. **Outcomes.** `outcome/step.rs`: `StepKind {FiatSent, Release, Cancel}` (action, canonical name, allowed replies), `StepInput::validate`,
   `StepResult` words, `StepData` (order id, result, trade pubkey, optional `next_trade_index`), `from_reply`.
2. **Operation.** `commands/order/step.rs`: lookup, exchange (no index, no next trade key), the room test (`Store::range_has_room`) only to add the `range_not_continued` warning on an accepted `fiat-sent`/`release`, mapping, status update with `local_status_stale`.
3. **Dispatch and grammar.** `OrderCommands::{FiatSent, Release, Cancel}(StepArgs)`; three registry entries (`JsonSupport::Write`); `cli/step.rs`;
   `parse_human/step.rs`; legacy variants removed from `commands.rs`/`legacy.rs`; `send_msg.rs` trimmed of the three arms and
   `create_next_trade_payload`/child-order wait if no other command uses them.
4. **Discovery.** Three command-scoped schemas and fixtures, regenerated discovery.

## Risks

- **Range orders**: without the next trade key the node ends the range after the slice; the warning makes it visible. Spec 008 (required)
  reserves the index, sends the key and gives the child order a local row (a placeholder keyed by trade index, linked to the order id when its
  `new-order` message is seen).
- **Cooperative cancel** has two steps from two parties; the CLI shows only the step it caused.
- **Release** is irreversible; the confirmation rule and the `uncertain` status are the protection.
- **MCP**: `fiat_sent`, `release`, `cancel` tools stay broken until migrated.

## Complexity Tracking

| Addition | Why | Simpler option rejected |
| --- | --- | --- |
| One operation for three steps | identical lookup/exchange/update; only the reply set and payload differ | three near-identical files |
