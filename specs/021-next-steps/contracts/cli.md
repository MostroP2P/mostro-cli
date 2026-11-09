# CLI Contract: Next Steps Of Message Records

No grammar change. Applies to `message list` (`message.list`) and to the records of `order watch` (`order.watch`).

## Behaviour (unit 1)

- A `payment-failed` message with its payload is the payload kind `payment_failed` (`payment_attempts`, `payment_retries_interval`,
  decimal strings). `next_step` is `null` while the node retries, `add_invoice` at `payment_attempts` `"0"`.
- `next_step` and the payload `kind` are open enums within `schema_version` `"1"`: see the versioning rule in `docs/commands.md`.
- A `next_step` is what its message asked when the node sent it. Check `order info` before acting, and act on each `event_id` once.

## Behaviour (unit 2)

- `hold-invoice-payment-accepted` gives `fiat_sent` (act only while `order info` says `active`); `rate` gives `rate` (act once the order
  is `success`, or `settled-hold-invoice` for a seller). `fiat-sent-ok`, `purchase-completed` and `hold-invoice-payment-settled` stay
  `null` (`release` on the seller's `fiat-sent-ok` is unit 3).
- `order watch` ends at `rate` as well; an update that holds `purchase-completed` or `hold-invoice-payment-settled` without a `rate`
  waits for it up to 10 s, never past `--timeout`, then reports `ended` either way. The end line still names the end action.

Proposed outcomes: [examples.json](examples.json) (`payment_failed_while_the_node_retries`, `messages_of_the_trade_steps`), checked against
`docs/schemas/message-list-v1.json` and the generated schema, and reproduced by the owned constructors.

## Human output (unit 1)

Next lines, with `ID` the record's order id (`<id>` when the record names none):

- `pay_bond_invoice`: "pay the bond invoice above with any Lightning wallet"
- `add_invoice`: "send an invoice for the sats: `order add-invoice --order ID --invoice <invoice>`"
- `pay_hold_invoice`: "pay the hold invoice above with any Lightning wallet"
- `add_bond_invoice`: "send an invoice for your share: `bond add-invoice --order ID --invoice <bolt11>`"
- `fiat_sent` (unit 2): "send the fiat, then: `order fiat-sent --order ID`"
- `rate` (unit 2): "rate your counterparty: `order rate --order ID --rating <1-5>`"

The `payment_failed` payload line:

- N at least 1: "🔁 Payment failed: the node will retry the payment up to N more times, every M seconds" ("1 more time", "every 1
  second"); no Next line.
- N = 0: "🔁 Payment failed: the node will not retry the payment", then the `add_invoice` Next line.
