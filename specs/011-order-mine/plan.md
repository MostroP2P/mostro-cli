# Implementation Plan: order mine

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented with Linux evidence on isolated stores.

## Design

1. **Store** (`store/rows.rs`): `Store::order_rows`, a read-only query of id, kind, status, trade key and creation time (`open_read_only`, no
   file created); `StoredOrder` keeps the key's public half only.
2. **Operation** (`commands/order/mine.rs`): the rows first, then `Store::trade_keys(200)` (the derivation `message list` uses; an index is
   reserved before its row is saved and only grows, so keys read after the rows cover them); match each row's key to an index, sort, map to `OrderMineData`; `trade_keys_limited` when an unmatched row may be older than the derived keys.
3. **Outcome** (`outcome/mine.rs`): `MineOrder::from_row` (refuses an unknown id, side or status), `OrderMineData`, error and warning
   lists; read stages.
4. **Command**: `OrderCommands::Mine`, registry (`JsonSupport::Read`), `cli/mine.rs` through `write::finish`, `parse_human/mine.rs`, discovery.

## Risks

- Deriving 200 keys costs time in a debug build (the PBKDF2 of the mnemonic runs per key); it is the cost `message list` already pays.
- A row whose key is foreign to the mnemonic gets `trade_index: null`; that is reported, not repaired.
