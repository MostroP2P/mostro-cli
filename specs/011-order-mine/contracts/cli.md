# CLI Contract: order mine

```text
mostro-cli order mine [--output human|json]
```

Canonical name `order.mine`. Reads the local store only (`MCLI_DIR`, else `~/.mcli`), read-only; needs no node, relay, Mostro key, stdin or
`--yes`. Root options are ignored. Data and outcomes: [data model](../data-model.md).

## Human output

A header with the count, then one line per order: id, side, status, trade index and trade key; "No local orders" when there are none.
`--verbose` prints one line `verbose: order.mine <code or ok>`.

## Stable errors

`invalid_arguments`, `identity_missing`, `store_unreadable`, `internal_error`. Statuses `ok` and `error` only.
