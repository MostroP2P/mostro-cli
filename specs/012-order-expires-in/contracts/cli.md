# CLI Contract: Order Expiry In Seconds

Amends the [order new contract](../../003-order-new-json/contracts/cli.md).

```text
mostro-cli order new ... [--expires-in SECONDS | --expiration-days DAYS] ...
```

`--expires-in` takes 1 to 31536000 seconds and asks the node for `expires_at = now + SECONDS`; the node lowers it to its
`max_expiration_days`. It cannot be combined with `--expiration-days` (`invalid_arguments`, exit 2, before any effect). Discovery lists the
input property `expires-in` on `order.new`.
