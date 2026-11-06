# CLI Contract: Add Invoice

Builds on the [order-take contract](../../004-order-take-json/contracts/cli.md) (streams, exit codes, confirmation, `uncertain`, one
document, diagnostics on stderr).

## Grammar

```text
mostro-cli order add-invoice --order UUID --invoice INVOICE [--yes] [--output human|json]
mostro-cli bond add-invoice  --order UUID --invoice BOLT11  [--yes] [--output human|json]
```

`addinvoice`, `addbondinvoice`, `-o`, `--order-id`, `-i` are removed with no alias. Canonical JSON names: `order.add-invoice`, `bond.add-invoice`.
`bond` is a new command group. Stdin is never read in JSON mode. (Amended 2026-11-05 by [spec 010](../../010-invoice-stdin/contracts/cli.md): except the invoice of `--invoice -`, which requires `--yes` in both modes.)

## Confirmation

As `order take`: JSON without `--yes` is `invalid_arguments`, exit 2, nothing done. Human mode previews the command, the order id and the trade
public key (never the invoice) and asks.

## Outcomes

See the [data model](../data-model.md). The trade index is never advanced. `uncertain` means the node may hold the invoice: the program does not
retry; the caller checks `order info --order <id>` (or, for a bond, waits for the payout) before sending again.

## Human output

One block: the order id and a sentence for the result; for `payment_accepted` the order. Node text is escaped. `--verbose` prints one line
`verbose: <command> <code or ok>`; diagnostics never include keys, relays, arguments or invoices.

## Stable errors

Those of `order take` plus `order_unknown`.

Amended 2026-11-05 (review of spec 010): an `invalid_arguments` caused by the invoice value itself carries `"reason": "invalid_invoice"`
(`order take-sell` only among the takes); every other `invalid_arguments` carries no reason. See the
[command reference](../../../docs/commands.md#machine-output-schema-v1).
