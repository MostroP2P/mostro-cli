# Research: Order New

Read 2026-10-29. Sources: the workspace `mostro` checkout (tag v0.20.1 plus three commits: `src/app.rs`,
`src/app/order.rs`, `src/util.rs`, `src/app/bond/flow.rs`), the legacy `src/cli/new_order.rs`, `src/util/storage.rs`
and `src/db.rs` of this repository, and the order-info exchange. Nothing here was checked against a running daemon
(regtest pins v0.19.3).

## What the daemon does with a `new-order`

1. `check_trade_index` (before dispatch): for a known identity, a `trade_index` of 1 or more must be **greater than** the
   user's `last_trade_index` or the node answers `CantDo(invalid_trade_index)`; the inner signature must verify against the
   sender (`invalid_signature`). For an unknown identity, index 0 is refused (`invalid_trade_index`); the identity must differ
   from the trade key (the user row is created from the first request).
2. `order_action` validates, in order: the invoice (`invalid_invoice`), accepted currency, a half-specified range
   (`invalid_amount`), the fiat amount, the sats amount, range limits, zero-amount-with-premium, the quote in sats. Each failure
   is a `CantDo(reason)`. Only after all checks pass does it set the user's `last_trade_index` to the request's index. **A
   refused request therefore does not consume the index on the node.**
3. Without a maker bond the order is created `pending` and the node sends `Action::NewOrder` with `Payload::Order` (the
   order with `id` set; the message `id` is also the order id), echoing the request id, **addressed to the trade key** (not
   the identity key). Refusals (`CantDo`) are addressed to the trade key too.
4. With a maker bond configured the order is parked `waiting-maker-bond` and the node sends a dedicated `pay-bond-invoice`
   message instead; the order is not on the book until the bond is paid. The order-info exchange allows only `orders` and
   `cant-do`, so such a reply is `reply_malformed` today. This spec reports it as its own error (`bond_required`).
5. Publishing the order to the relays can fail after the ack was queued; that is the node's business and not visible to the client.

## What the legacy CLI does (and what is wrong for an agent)

- `init_context` reads or **creates** the identity, then **reserves** the next index (`get_next_trade_keys` only computes
  `last_trade_index + 1`; nothing is written yet). The index is written locally only by `save_order` after an accepted
  reply, together with the order row (`Order::new`). A failed save therefore leaves the local index stale while the node
  already consumed it: the next `neworder` is refused with `invalid_trade_index` until `getlasttradeindex` resyncs.
- Prompt on stdin, `process::exit`, a call to api.yadio.io for market-priced currencies, a human preview and result printed
  inline; the reply goes through the shared `wait_for_dm`/`print_dm_events` with an unrequested-`NewOrder` tolerance for range
  children (out of scope here).

## Design consequences

- The exchange must carry a **separate identity and trade key**: the request is wrapped with the identity as seal signer and
  the trade key as author and inner signer (`wrap_message_with(.., identity, trade, ..)`), the wait is armed for the **trade key**
  as recipient and decrypts with it. Order info passes the identity for both and keeps working.
- The local index must advance when the request was published and not provably rejected, otherwise a successful or ambiguous
  publication reuses the same key and index next time. Gaps are harmless (the node only needs `index > last`). A provably
  rejected request (`CantDo` refusal, or not published) leaves the index alone. The save and the index update are one SQLite
  transaction, so a half-saved state is impossible.
- A failed save after an accepted reply is `uncertain`. The recovery data is **public**: the order id, the trade index and the
  trade public key. The trade private key is derivable from the mnemonic and the index (`account trade-key`), so no key
  material is printed.
- The write path is the first migrated command that writes `mcli.db`. It needs a small read-write store module separate from
  `identity.rs` (which stays read-only) and must still create nothing when the identity is absent.
- Idempotency: the CLI cannot detect a duplicate order created by an `uncertain` run; the contract tells callers to run
  `order list` (own orders appear in the book) before trying again.

## Unverified

Everything above is source reading. The regtest probe (separately authorized, regtest identities only) must record: the accepted
reply shape and addressing, each `CantDo` reason for bad currency, range, invoice and index, behavior when the node is bonded,
and the index state after a refusal.
