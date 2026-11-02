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

See the [data model](../data-model.md). The trade index is never advanced and no next trade key is sent. `uncertain` means the node may have
acted: the program does not retry; the caller checks `order info --order <id>` before sending again.

## Range orders

On a range order of the user's with room left, an accepted `fiat-sent` or `release` carries the warning `range_not_continued`: the node does not
create an order for the rest of the range. Continuing the range is spec 008.

## Human output

One block: a title for the result, the order id, the trade public key and a next-step sentence. The warnings `range_not_continued` and
`local_status_stale` are fixed lines on stderr. `--verbose` prints one line `verbose: <command> <code or ok>`; diagnostics never include keys,
relays or arguments.

## Stable errors

Those of `order add-invoice` (`invalid_arguments`, `configuration_*`, `identity_*`, `store_unwritable`, `protocol_unsupported`, `pow_required`,
`relay_*`, `request_rejected`, `node_refused` with the node's reason, `order_declined`, `order_unknown`, `internal_error`) and, for `uncertain`,
`reply_timeout`, `reply_malformed`, `request_rejected`, `relay_read_failed`, `pow_required`.
