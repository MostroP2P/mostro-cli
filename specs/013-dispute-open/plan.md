# Implementation Plan: dispute open

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented with Linux evidence against the scripted node.

## Design

1. **Shared request on a held order** (`commands/order/held.rs`): the order's stored key, one request through `msg::exchange` correlated by
   request id, `unsent` / `refused` / `uncertain` outcomes and the optional local status update, parameterised by the action, payload, allowed
   replies and a reply mapper. Used by `dispute open` and `order rate` (spec 014). The trade steps keep their own operation (the range
   continuation reserves an index), unchanged.
2. **Outcome** (`outcome/dispute.rs`): `DisputeOpenData`, error lists (those of the steps), `Stages::Update`.
3. **Command**: `Commands::Dispute { DisputeCommands::Open }`, registry `["dispute","open"]` (`JsonSupport::Write`), `cli/held.rs` dispatch
   (shared with `order rate`), `parse_human/held.rs`; the legacy arm, `run_simple_order_msg` and `cli/send_msg.rs` (which served only the legacy
   `dispute` by then) removed.
4. **Scripted node**: `step_daemon` gains `dispute` with the source's rules (status, parties, a second dispute is silent).
