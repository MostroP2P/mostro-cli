# CLI Contract: Order Take

Builds on the [order-new contract](../../003-order-new-json/contracts/cli.md) (streams, exit codes, confirmation, `uncertain`, one document, diagnostics
on stderr) and the [pilot envelope](../../001-order-list-json/contracts/cli.md).

## Grammar

```text
mostro-cli order take-sell --order UUID [--invoice INVOICE] [--amount FIAT] [--yes] [--output human|json]
mostro-cli order take-buy  --order UUID [--amount FIAT] [--yes] [--output human|json]
```

`takesell`, `takebuy`, `-o`, `--order-id`, `-i`, `-a` and the positional forms are removed with no alias. Canonical JSON names:
`order.take-sell` and `order.take-buy`. The side is the side of the order taken: `take-sell` takes a sell order (the taker buys sats),
`take-buy` takes a buy order (the taker sells sats). Stdin is never read in JSON mode.

## Confirmation

As `order new`: JSON without `--yes` is `invalid_arguments`, exit 2, nothing done. Human mode shows a preview (action, order id, amount if any,
whether an invoice was given, the trade index and key; never the invoice) and asks; blank or `y` accepts; `n`, anything else and closed stdin
are `order_declined` (exit 1, nothing done). `--yes` skips the prompt in both modes.

## Outcomes

Statuses `ok`, `error`, `uncertain`; exit 0 for `ok`, 2 for `invalid_arguments`, 1 for everything else. The [data model](../data-model.md) fixes
the stages per case and the `next_step` per reply. `uncertain` means the node may hold the take: the program does not retry; the caller checks
`order info --order <id>` before trying again. The trade index is advanced whenever the take was published and not provably refused.

## Human output

After the preview, one block: the order, the next step in words, and, for `pay_hold_invoice`, the hold invoice on its own line so it can be
copied. Text from the node is escaped. Diagnostics (including `--verbose`: one line `verbose: order.take-sell <code or ok>`) never include keys, relays,
arguments, invoices or hold invoices.

## Stable errors

Inherited from `order new` (`invalid_arguments`, `configuration_*`, `identity_*`, `protocol_unsupported`, `pow_required`, `request_rejected`,
`node_refused` + reason, `reply_timeout`, `reply_malformed`, `relay_*`, `order_declined`, `bond_required`, `order_not_saved`, `store_unwritable`,
`internal_error`). None is added.
