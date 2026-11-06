# CLI Contract: Reply Deadline

```text
mostro-cli [--reply-timeout SECONDS] <command> ... [--reply-timeout SECONDS]
```

`SECONDS` is 5 to 300 (default 15). It is the deadline from publishing the request to the reply for `order new`, `order take-sell`,
`order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order info`, `dispute open`, `order rate` and
`bond add-invoice`; other commands accept and ignore it. A reply after the deadline is `reply_timeout` as before; the program never retries.
Discovery: `global_options` ([data model](../data-model.md)).
