# CLI Contract: order watch

```text
mostro-cli order watch --order UUID [--since MINUTES | --after CURSOR] [--timeout SECONDS] [--output human|json]
```

Canonical name `order.watch`. Human mode since unit 5, JSON since unit 6. Needs `MOSTRO_PUBKEY`,
`RELAYS`, the identity and the order's row. `--pow`, `--transport` and `--reply-timeout` do not apply: nothing is published.

- `--order`: an order in the local store; otherwise `order_unknown` before any relay.
- `--since`: 1 to 10080 minutes, default 30. Conflicts with `--after`.
- `--after`: a cursor returned by an earlier call, passed back verbatim; the call returns only records that one did not.
- `--timeout`: 1 to 86400 seconds, counted from the end of the first replay. JSON defaults to 30; human mode has no default.

Any invalid value exits 2 (`invalid_arguments`) before configuration, store or relay access, and is never shown.

With `--output json` a call returns one document at the first new records after the replays, or an empty success at the deadline, with
the next cursor. Human mode streams updates until the trade ends or the deadline passes, and exits 0 in both cases. Data, outcomes and
stable errors: [data model](../data-model.md). Proposed outcome: [examples.json](examples.json) (`new_records`); until unit 7 it is
checked against the `order.watch` output schema inside `docs/schemas/discovery-v1.json`. The ending actions and the `admin-settled` side rule: [spec](../spec.md) D9.

## Human output (unit 5)

A header with the order, the window and the counterparty (or "not known yet; the chat is followed once the escrow is locked"); "📭
Nothing yet; waiting for the node or the counterparty..." when the first update is empty; `message list`'s message block with its next
step; `order chat`'s chat block, escaped; a line when the counterparty is learned and one when older records may be missing; "🏁 The trade
ended: ACTION" at the end, or "⏰ Stopped at the watch deadline; the trade has not ended." at the deadline. Errors on stderr; the read
warnings are reported only in JSON. `--verbose` prints one line `verbose: order.watch <ok|code>`. Each update is rendered whole,
written and flushed; a stdout that cannot be written stops the watch with `error: the watch could not be written` (exit 1). Other
errors are one `error: <fixed message>` line (exit 1, 2 for arguments). Ctrl-C stops the process by its default action; no exit status is promised.
