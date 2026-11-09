# CLI Contract: order chat (proposed)

```text
mostro-cli order chat --order UUID [--message TEXT] [--since MINUTES] [--yes] [--output human|json]
```

Canonical name `order.chat`. Not available yet: unit 1 adds only the counterparty lookup. Unit 3 adds the command in human mode and unit 4
JSON. Needs `MOSTRO_PUBKEY`, `RELAYS`, the identity and the order's row. `--pow`, `--transport` and `--reply-timeout` do not apply.

- `--order`: an order in the local store; otherwise `order_unknown` before any relay.
- `--message`: one value, trimmed, 1 to 4096 bytes; `--message=<text>` for a text starting with `-`. JSON requires `--yes`; human mode
  sends without a prompt.
- `--since`: 1 to 10080 minutes, default 10080.

The counterparty is the order row's key when it parses and is not ours, used with no relay contacted; otherwise the one the newest
post-lock message the node sent to the order's trade key names, then cached in the row. Neither: `counterparty_unknown`. `getdmuser` and
`dmtouser` stay for orders without a local row. Data, outcomes and stable errors: [data model](../data-model.md).

## Human output (unit 3)

"💬 Order Chat", the order and the counterparty, then "📭 No chat messages" or one block per message (time, "🫵 You" or "👤 Counterparty",
the escaped text indented); a truncation line; "📤 Your message (id ...)" when one was sent, also on `uncertain`. Errors and advice on
stderr; `--verbose` prints one line `verbose: order.chat <ok|code>`.
