# CLI Contract: Message List

Builds on the [order-list contract](../../001-order-list-json/contracts/cli.md) (streams, exit codes, the read envelope, snapshot evidence, diagnostics on stderr).

## Grammar

```text
mostro-cli message list [--since MINUTES] [--order UUID] [--output human|json]
```

`getdm`, `-s`, `-f` and `--from-user` are removed with no alias. Canonical JSON name: `message.list`. No prompt, no `--yes`, no stdin.

## Behavior

Read-only: nothing is created or written, the store need not be writable, and a repeated call returns the same messages. The keys read are those of the trade indexes 1 to the
recorded last index (at most the newest 200); `--order` reads only that order's stored key. Only messages authored by the configured node and addressed to those keys are
returned, oldest first, at most 200. `--since` is 1 to 10080 minutes (default 30).

## Outcomes

Statuses `ok` and `error`; exit 0, 2 for `invalid_arguments`, 1 otherwise. An empty window is `ok`. See the [data model](../data-model.md).

## Human output

A header (window, keys read), then one block per record: time, action, order id and the details of its payload; an invoice the node sent is alone on its line so it can be
copied; `next_step` is stated in words. Text from the node is escaped. `--verbose` prints one line `verbose: message.list <code or ok>`; diagnostics never include keys, relays,
arguments, invoices or message text.

## Stable errors

Those of `order list` that apply to a read with an identity (`invalid_arguments`, `configuration_*`, `relay_*`, `read_*`, `internal_error`) plus `identity_missing`,
`identity_unreadable`, `order_unknown` and `store_unreadable`.
