# CLI Contract: Range Order Children

Builds on the [trade steps contract](../../007-trade-steps-json/contracts/cli.md) and the
[message list contract](../../006-message-list-json/contracts/cli.md) (streams, exit codes, one document, diagnostics on stderr).

## Grammar

```text
mostro-cli order sync [--since MINUTES] [--output human|json]
```

`--since` is 1 to 10080 minutes (default 10080). There is no `--yes` and no question: the command publishes nothing, and only adds rows for orders
the node announced to the user's own trade keys. Canonical JSON name: `order.sync`. It needs an existing local identity and a writable store; it never
creates either, and it does not look at `--pow`, `--transport`, `POW` or `TRANSPORT`.

## What it does

Reads the node's `new-order` messages for the user's derived trade keys (the newest 200) as `message list` does, and saves a row for every
announced order the store does not know: the order's terms, the key of the index it was announced to, and the user's own marker (so that a
range child can itself be continued). It never replaces a row, so a second run saves nothing.

## Range orders in the trade steps

On a range order the user made with room left, `release` of a sell order and `fiat-sent` of a buy order carry a fresh trade key at the next index.
See the [data model](../data-model.md) for the fields; the other commands are unchanged.

## Human output

A header with the window and what was read, then one block per order added (id, side, status, amount, fiat, trade index and key) and the next
command, or "nothing to add". `--verbose` prints one line `verbose: order.sync <code or ok>`.

## Stable errors

Those of `message list` without `order_unknown`, plus `store_unwritable`; and for `uncertain`, `order_not_saved` (some rows were saved and one was not;
run it again).
