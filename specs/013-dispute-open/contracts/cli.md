# CLI Contract: dispute open

Builds on the [trade steps contract](../../007-trade-steps-json/contracts/cli.md).

```text
mostro-cli dispute open --order UUID [--yes] [--output human|json]
```

Canonical name `dispute.open`. `dispute`, `-o` and `--orderid` (documented as `--order-id`) are removed with no alias. JSON requires `--yes` and never prompts; human mode
shows the order id and trade key and asks. One request, no trade index. Data and outcomes: [data model](../data-model.md). `uncertain` means the
node may have opened the dispute: check `order info --order <id>` before trying again (a second request is answered with silence).

## Stable errors

Those of the trade steps: `invalid_arguments`, configuration, identity, `protocol_unsupported`, `pow_required`, relay codes, `node_refused`,
`order_declined`, `store_unwritable`, `order_unknown`, `internal_error`; `uncertain` with `reply_timeout`, `reply_malformed`, `request_rejected`,
`relay_read_failed`, `pow_required`. Warning `local_status_stale`.
