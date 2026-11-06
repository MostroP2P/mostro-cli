# Implementation Plan: order rate

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented with Linux evidence against the scripted node.

Reuses the request on a held order of [spec 013](../013-dispute-open/plan.md) with no local status (`Stages::Delivery`). `outcome/rate.rs`
(`RateInput::validate`, `RateData`), `commands/order/rate.rs`, `OrderCommands::Rate`, registry `["order","rate"]`, `cli/held.rs`,
`parse_human/held.rs` (both renderers); `cli/rate_user.rs` and the legacy arm removed. `step_daemon` gains `rate-user` with the source's rules.
The rating is a JSON integer (as `last_trade_index` of `identity show`), bounded 1..=5 in the schema.
