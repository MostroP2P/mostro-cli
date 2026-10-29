# Implementation Plan: Order New

**Spec**: [spec.md](spec.md) | **Date**: 2026-10-29 | **Status**: Proposed design; no implementation, no runtime evidence.

## Summary

`order new` (replacing `neworder`) is the first migrated write: one signed `new-order` published through `msg::exchange`
(extended to a separate trade key), the reply selected by correlation, the order and the trade index saved in one transaction,
and an `uncertain` status when the node may hold an order the CLI cannot prove. Evidence and decisions: [research.md](research.md).

## Technical Context

Same as order info: Rust 2021, MSRV 1.94, no new dependency, test side on the loopback relay, the scripted fake daemon and the
process harness. New: a read-write storage module for the order row and index (`sqlx`, already a dependency). Files stay at or
below 400 production lines; imports at the top; tests before code; no live node, public relay or regtest without authorization.

## Constitution Check

| Principle | Compliance |
| --- | --- |
| Imports, size, reuse | new files `commands/order/new.rs`, `store.rs` (write path), `outcome/new.rs`, `parse_human/order_new.rs`; reuse exchange, identity, envelope, schema generator, `OrderRecord`, the table helpers; `cli/migrated.rs` stays below 400 (split first if needed) |
| Boundaries | wrapper returns a typed outcome; renderers pure; the only prompt lives in `cli/migrated.rs`, never in `commands` or `msg`; all transport in `msg`; storage writes only in the store module, called by the wrapper after a reply |
| Evidence | legacy `neworder` characterized first; scripted daemon from the source; regtest probe separate |
| Safety | a write command: confirmation rule, one publish, no retry, index never reused after publication, no key material in outputs, secrets in sentinel tests |

## Design

1. **Exchange extension.** `Outbound` gains the identity key next to the trade key (`signer` becomes `trade`, plus `identity`); wrap with
   identity as seal signer and the trade key as author; the wait is armed and decrypted for the trade key. `check_protocol`, work and
   cleanup are unchanged. Order info passes the same key for both; its tests stay green. `Expect` allows `new-order` and refusals; the bond
   reply is returned to the wrapper (`Action::PayBondInvoice`) instead of being treated as malformed.
2. **Store.** `src/store.rs`: open `mcli.db` read-write only after validation and confirmation, never create it; `reserve()` reads
   `last_trade_index`; `commit_published(index, order)` writes the order row and the index in one transaction; `advance(index)` for the
   ambiguous cases. Typed errors map to `store_unwritable` (before publishing) or `order_not_saved`/persistence stages (after).
3. **Wrapper.** `order_new(input, config, identity, store, options, limits) -> NewOutcome`: derive the trade key for `last+1`, build the
   `SmallOrder`, `exchange`, map `NewOrder` to ok + save, `CantDo` to `node_refused`, bond reply to `bond_required`, anything else to
   `reply_malformed`; timeouts and ambiguities to `uncertain` with the index advanced.
4. **Dispatch.** `cli/migrated.rs` resolves configuration, then identity, then (human) the preview and prompt or (JSON) the `--yes` check, then
   the wrapper. Everything that can fail locally fails before the store is opened for writing.
5. **Discovery.** `OrderCommands::New` replaces `NewOrder`, registry `order.new` becomes `JsonSupport::Request`, command-scoped
   schemas allow `ok`, `error` and `uncertain` for this command only; `order-new-v1.json` is a new generated fixture; the other fixtures do not change.

## Risks

- **Daemon version**: v0.20.1 source vs regtest v0.19.3; the probe decides claims.
- **Duplicate orders**: an `uncertain` run may have created an order; documented, not detectable by the CLI.
- **MCP**: `create_order` stays broken until migrated.
- **Index policy** (advance on ambiguity) is a deliberate departure from legacy; it is tested at the process level with a forced save failure.
- **Bonded nodes**: the bond is not paid by this command; the outcome says so.
