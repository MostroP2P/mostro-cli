# CLI Contract: order chat (implemented)

```text
mostro-cli order chat --order UUID [--message TEXT] [--since MINUTES] [--yes] [--output human|json]
```

Canonical name `order.chat`, `JsonSupport::Read` since unit 4. Output schema
[docs/schemas/order-chat-v1.json](../../../docs/schemas/order-chat-v1.json); the input schema is in the discovery entry (D9). Needs
`MOSTRO_PUBKEY`, `RELAYS`, the identity and the order's row. `--pow`, `--transport` and `--reply-timeout` do not apply.

- `--order`: an order in the local store; otherwise `order_unknown` before any relay.
- `--message`: one value, trimmed, 1 to 4096 bytes; `--message=<text>` for a text starting with `-`. JSON requires `--yes`; human mode
  sends without a prompt.
- `--since`: 1 to 10080 minutes, default 10080.

The counterparty is the order row's key when it parses and is not ours, used with no relay contacted; otherwise the one the newest
post-lock message the node sent to the order's trade key names, then cached in the row. Neither: `counterparty_unknown`. `getdmuser` and
`dmtouser`, which also need the order's row, stay the fallback for that case when the caller knows the key. Data, outcomes and stable
errors: [data model](../data-model.md). JSON outcomes, reproduced by the outcome constructors and checked against the checked-in output
schema (`tests/integration_tests.rs`, `chat_examples_conform_and_are_reproduced`): [examples.json](examples.json)
(`conversation_read`, `message_sent`, `relays_refused`, `send_uncertain`, `counterparty_unknown`, `message_without_yes`).

## Human output (implemented, unit 3)

"💬 Order Chat", the order and the counterparty, then "📭 No chat messages" or one block per message (time, "🫵 You" or "👤 Counterparty",
the escaped text indented); a truncation line; "📤 Your message (id ...)" when one was sent, also on `uncertain`. Errors and advice on
stderr; `--verbose` prints one line `verbose: order.chat <ok|code>`.
