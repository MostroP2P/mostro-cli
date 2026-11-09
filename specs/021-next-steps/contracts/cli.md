# CLI Contract: Next Steps Of Message Records

No grammar change. Applies to `message list` (`message.list`) and to the records of `order watch` (`order.watch`).

## Behaviour (unit 1)

- A `payment-failed` message with its payload is the payload kind `payment_failed` (`payment_attempts`, `payment_retries_interval`,
  decimal strings). `next_step` is `null` while the node retries, `add_invoice` at `payment_attempts` `"0"`.
- `next_step` and the payload `kind` are open enums within `schema_version` `"1"`: see the versioning rule in `docs/commands.md`.
- A `next_step` is what its message asked when the node sent it. Check `order info` before acting, and act on each `event_id` once.

Proposed outcome: [examples.json](examples.json) (`payment_failed_while_the_node_retries`), checked against
`docs/schemas/message-list-v1.json` and the generated schema, and reproduced by the owned constructors.

## Human output (unit 1)

Next lines, with `ID` the record's order id (`<id>` when the record names none):

- `pay_bond_invoice`: "pay the bond invoice above with any Lightning wallet"
- `add_invoice`: "send an invoice for the sats: `order add-invoice --order ID --invoice <invoice>`"
- `pay_hold_invoice`: "pay the hold invoice above with any Lightning wallet"
- `add_bond_invoice`: "send an invoice for your share: `bond add-invoice --order ID --invoice <bolt11>`"

The `payment_failed` payload line:

- N at least 1: "🔁 Payment failed: the node will retry the payment up to N more times, every M seconds" ("1 more time", "every 1
  second"); no Next line.
- N = 0: "🔁 Payment failed: the node will not retry the payment", then the `add_invoice` Next line.
