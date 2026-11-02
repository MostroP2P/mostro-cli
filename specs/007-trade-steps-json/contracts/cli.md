# CLI Contract: Trade Steps

Builds on the [add-invoice contract](../../005-invoice-json/contracts/cli.md) (streams, exit codes, confirmation, `uncertain`, one document,
diagnostics on stderr).

## Grammar

```text
mostro-cli order fiat-sent --order UUID [--yes] [--output human|json]
mostro-cli order release   --order UUID [--yes] [--output human|json]
mostro-cli order cancel    --order UUID [--yes] [--output human|json]
```

`fiatsent`, `release`, `cancel`, `-o`, `--orderid` and `--order-id` are removed with no alias. Canonical JSON names: `order.fiat-sent`,
`order.release`, `order.cancel`. Stdin is never read in JSON mode.

## Confirmation

As `order take`: JSON without `--yes` is `invalid_arguments`, exit 2, nothing done. Human mode previews the command, the order id and the trade
public key and asks; the `release` preview says it cannot be undone.

## Outcomes

See the [data model](../data-model.md). The trade index is advanced only for the key of a range order's maker step. `uncertain` means the node may have
acted: the program does not retry; the caller checks `order info --order <id>` before sending again.

## Range orders

See [spec 008](../../008-range-children/spec.md). On a range order the user made with room left, the maker's own step (`release` of a sell order,
`fiat-sent` of a buy order) carries a fresh trade key; the data reports the reserved `next_trade_index`, and after a `release` `next_step` is
`sync_child_order`. Elsewhere (a taker, a fixed order, an exhausted range, `cancel`) nothing is sent and no index is spent.

## Human output

One block: a title for the result, the order id, the trade public key and a next-step sentence. The warnings `trade_index_not_recorded` and
`local_status_stale` are fixed lines on stderr. `--verbose` prints one line `verbose: <command> <code or ok>`; diagnostics never include keys,
relays or arguments.

## Stable errors

Those of `order add-invoice` (`invalid_arguments`, `configuration_*`, `identity_*`, `store_unwritable`, `protocol_unsupported`, `pow_required`,
`relay_*`, `request_rejected`, `node_refused` with the node's reason, `order_declined`, `order_unknown`, `internal_error`) and, for `uncertain`,
`reply_timeout`, `reply_malformed`, `request_rejected`, `relay_read_failed`, `pow_required`.
