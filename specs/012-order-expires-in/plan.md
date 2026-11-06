# Implementation Plan: Order Expiry In Seconds

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented with Linux evidence against the scripted node.

## Design

- `NewArgs.expires_in: Option<i64>` with Clap `conflicts_with = "expiration_days"` (an explicit `--expiration-days` only; its default does not
  conflict), help text naming the range.
- `OrderRequest.expires_in`, `OrderNewInput::validate` checks 1..=31536000 (`InvalidOrder::ExpiresIn`) and refuses both at once (defense in
  depth); `small_order(now)` sets `expires_at = now + seconds`.
- The preview line shows the seconds; discovery picks the property up from Clap.
