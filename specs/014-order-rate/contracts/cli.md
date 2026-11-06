# CLI Contract: order rate

```text
mostro-cli order rate --order UUID --rating 1-5 [--yes] [--output human|json]
```

Canonical name `order.rate`. `rate`, `-o`, `--orderid` (documented as `--order-id`), `-r` and `--rating` of the legacy form are removed with no alias. JSON requires `--yes`. One request with the order's
stored key; nothing is written locally. Data and outcomes: [data model](../data-model.md). Stable errors as
[dispute open](../../013-dispute-open/contracts/cli.md), without the warning.
