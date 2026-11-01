# Implementation Plan: Add Invoice

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-01 | **Status**: Implemented (slices 1-6) with Linux evidence against a scripted fake daemon; behavior against a real daemon is unverified.

## Summary

Two write commands, one operation parameterised by the kind: look up the order's stored trade key, publish one signed reply with
`msg::exchange`, map the correlated answer, update the local status (`order` only). Reuse `cli/write.rs`, `Store`, the envelope, the schema
generator and the invoice grammar of `take-sell`. Evidence and decisions: [research.md](research.md).

## Technical Context

Rust 2021, MSRV 1.94, no new dependency. Test side: loopback relay, the scripted fake daemon extended with `add_invoice_daemon` (accepting the
three replies, the refusals `invalid_peer`, `invalid_invoice`, `not_allowed_by_status`, and a bond-payout mode with and without the ack, the ack
sent with no request id) and the process harness. Files at or below 400 production lines; imports at the top; tests before code; no live node,
public relay or regtest without authorization.

## Design

1. **Store.** `Store::trade_for_order(id)` returns the order's trade keys (private) or `order_unknown`; `Store::set_status(id, status)` for the update.
2. **Exchange.** `Expect` gains an optional correlation by order id (`by_order`), used only by these commands; the request-id path is unchanged.
3. **Input and data.** `outcome/invoice.rs`: `InvoiceInput::validate` (UUID, invoice by kind), data and result enums for both names.
4. **Operation.** `commands/order/add_invoice.rs` and `commands/bond/add_invoice.rs` (or one module with a kind), using the `write::Sent` helpers
   minus the index logic (no index spent).
5. **Dispatch and grammar.** `OrderCommands::AddInvoice`, new `BondCommands::AddInvoice`; registry paths `["order","add-invoice"]` and
   `["bond","add-invoice"]` with the existing canonical names; `cli/invoice.rs`; `parse_human/invoice.rs`; legacy files removed.
6. **Discovery.** Command-scoped schemas, `order-add-invoice-v1.json`, `bond-add-invoice-v1.json`, regenerated discovery.

## Risks

- Order-id correlation is weaker than request-id correlation; mitigated by author, addressee and allowed actions, and tested with a decoy.
- `bond add-invoice` is unprobeable on regtest v0.19.3; claims stop at the scripted daemon.
- MCP: `add_invoice` stays broken until migrated.
