# CLI Contract: Order New

Builds on the [order-info contract](../../002-order-info-json/contracts/cli.md) (streams, exit codes, one document,
diagnostics on stderr) and the [pilot envelope](../../001-order-list-json/contracts/cli.md).

## Grammar

```text
mostro-cli order new --side buy|sell --currency CODE --fiat-amount N [--fiat-amount-max M]
                     --payment-method TEXT [--sats N] [--premium N] [--invoice BOLT11]
                     [--expiration-days N] [--yes] [--output human|json]
```

`neworder` and its short flags (`-k -c -f -m -a -p -i -e`), the `100-200` range syntax and a positional form are removed with
no alias. `--yes` is local to this command for now (the roadmap's global `--yes` waits for a command that needs it elsewhere).
Canonical JSON name: `order.new`. Stdin is never read in JSON mode.

## Confirmation

- JSON mode without `--yes`: `invalid_arguments`, exit 2, nothing done (no index, no write, no network).
- Human mode: preview, then the prompt; blank or `y` accepts, `n` is `order_declined` (exit 1, nothing done), any other answer or
  closed stdin is not approval (`order_declined`). `--yes` skips the prompt in both modes.

## Outcomes

Statuses `ok`, `error` and `uncertain` (the envelope's third value, not used before). Exit 0 for `ok`, 2 for `invalid_arguments`,
1 for everything else including `uncertain`. The table in the [data model](../data-model.md) fixes the stages per case.

`uncertain` means the node may hold a live order. The CLI does not retry. A caller reads `order list` (own orders) before trying
again; the trade index is advanced locally whenever the request was published and not provably rejected, so the next attempt
uses a fresh key.

## Human output

The order preview before confirmation uses the labelled lines of the existing preview; after the outcome the familiar "order
created" lines with id, trade index and trade public key. Text from the node is escaped. Diagnostics never include keys, relays,
arguments or the invoice.

## Stable errors

Inherited: `invalid_arguments`, `configuration_*`, `identity_missing`, `identity_unreadable`, `protocol_unsupported`,
`pow_required`, `request_rejected`, `node_refused` (+ reason), `reply_timeout`, `reply_malformed`, `relay_*`, `internal_error`.
Added (all exit 1, scoped to this command's schema): `order_declined`, `bond_required`, `order_not_saved`, `store_unwritable`.
