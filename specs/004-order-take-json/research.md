# Research: Order Take

Read 2026-10-30. Sources: the workspace `mostro` checkout (tag v0.20.1 plus three commits: `src/app/take_sell.rs`, `src/app/take_buy.rs`,
`src/app.rs`, `src/util.rs`), the legacy `src/cli/take_order.rs`, `src/parser/dms.rs` and the code of `order new` in this repository.
Nothing here was checked against a running daemon (regtest pins v0.19.3).

## What the daemon does with a take

Both takes share the front of `order new`'s path: `check_trade_index` runs first (a known identity needs an index above its
`last_trade_index` and a valid inner signature; an unknown identity may not use index 0), and the node sets the user's
`last_trade_index` only after every check of the action passed. **A refused take therefore does not consume the index on the node**,
as for a new order.

`take_sell_action` (the taker buys sats):
1. `PendingOrderExists` if the taker (by identity) already has a pending order as buyer; the order must be a sell order, `pending` (or
   `waiting-taker-bond`) and inside its take window, else `InvalidOrderStatus`; the sender may not be the maker (`not_sent_from_maker`).
2. A range order needs `--amount` inside `[min, max]`, else `OutOfRangeSatsAmount`; a market-priced order needs a fresh price
   (`PriceTooStale`). A buyer invoice in the payload is validated against the amount and fee (`InvalidInvoice`).
3. Then the index is recorded and:
   - **taker bond configured**: `PayBondInvoice` to the taker (order parked), nothing else;
   - **no invoice**: the order becomes `waiting-buyer-invoice`; the taker gets **`AddInvoice` with `Payload::Order`** (the order, amount net of
     the fee, `id` set, trade index echoed), the maker gets `WaitingBuyerInvoice`;
   - **invoice present**: a hold invoice is created; the **maker** gets `PayInvoice` with the hold invoice and the taker gets
     **`WaitingSellerToPay` with no payload** (the message `id` is the order id). The taker's request id is echoed on all of them, but the
     `PayInvoice` is addressed to the maker, so the taker's wait never sees it;
   - **Cashu escrow enabled**: an escrow request goes out instead (`show_cashu_escrow_request`); its shape is not modeled here.

`take_buy_action` (the taker sells sats): the same front with `seller_has_pending_order`, the order must be a buy order, then the index is
recorded and: taker bond configured: `PayBondInvoice` to the taker; Cashu: escrow request; otherwise a hold invoice is created and the taker (the
seller) gets **`PayInvoice` with `PaymentRequest(Some(order), hold_invoice, None)`**, the maker gets `WaitingSellerToPay`.

So the replies a taker can see are `add-invoice` (take-sell, no invoice), `waiting-seller-to-pay` (take-sell, invoice), `pay-invoice` (take-buy),
`pay-bond-invoice` (either) and `cant-do` (either), all echoing the request id and addressed to the trade key.

## What the legacy CLI does (and what is wrong for an agent)

- `init_context` creates the identity if absent and computes the next index; the index and the order row are written only by `save_order`
  after a reply that carries an order (`add-invoice`, `pay-invoice`, `pay-bond-invoice`). A failed save leaves the local index behind the node.
- A take-sell with an invoice gets `waiting-seller-to-pay` with no order, so nothing saves it; `execute_take_order` then calls
  `save_taken_sell_order`, which reads the **public book** (a second network read) to build the row, or saves only the id and keys, and reports
  `accepted_take_local_save_failed` if that fails (#204).
- No confirmation, prints a table of the arguments (including the invoice), prints inside the reply handler, and `process::exit` paths exist in
  the shared reply handling.
- The invoice payload: a Lightning address is passed through; a BOLT-11 string is validated and, if invalid, printed and sent anyway; the
  amount rides in the same `PaymentRequest` or as `Payload::Amount` (`take-buy` sends only an amount; `take-sell` without invoice and amount
  sends `Amount(0)`).

## Design consequences

- Both commands are one operation parameterised by the side; their payload is built from validated input (an invalid invoice fails locally
  instead of being sent). `Amount(0)` for "no amount" is kept only where the node expects it (to be confirmed by the regtest probe; the plan
  keeps the legacy bytes and pins them in a characterization test).
- The reply set is small and each member maps to one `next_step`: `add_invoice`, `wait_for_seller_payment`, `pay_hold_invoice`. A bond request is
  `bond_required`; a refusal is `node_refused`; anything else (including a Cashu escrow request) is `reply_malformed`, `uncertain`.
- The local row comes from the reply order when there is one (as the new-order save does, status taken from the node) and is minimal otherwise
  (decided in the spec), saved with the index by the existing `Store::save_order`. The trade key stored with the row is what the later commands
  (`addinvoice`, `fiatsent`, `release`) need.
- The hold invoice is data the caller must pay. It is not a secret of the program but it is sensitive (paying it commits funds): it appears only
  in the result, never in diagnostics, `--verbose` or errors.
- Idempotency: after an `uncertain` take the caller must check the order's state (`order info --order <id>` shows the user's own orders; a taken order
  appears there) before trying again; a second take of the same order is refused by the node (`InvalidOrderStatus` or `PendingOrderExists`).

## Unverified

Everything above is source reading. The regtest probe (separately authorized, regtest identities only) must record: each reply and its
addressing for both sides, with and without an invoice and with a range amount; whether `waiting-seller-to-pay` can answer a take-sell without
an invoice; the refusal reasons for own order, taken order, expired window, out-of-range amount and pending order; the index after each refusal;
the behavior on a bonded node and with Cashu enabled.
