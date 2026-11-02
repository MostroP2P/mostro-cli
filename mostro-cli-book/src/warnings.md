# Warning codes

Warnings never change `status`. They tell you that the result is valid but something
deserves attention. At most one warning per code appears in a document.

| Code | Message | Emitted by | Meaning and what to do |
| --- | --- | --- | --- |
| `invalid_event_timestamp` | One or more event timestamps cannot be shown as a UTC date. | every command (in practice the relay reads) | A `created_at` is `null` because the event time is out of range; the `_unix` value is still exact. Display "invalid date". |
| `cleanup_incomplete` | Relay cleanup did not finish within its time budget. | every command | Unsubscribing/disconnecting took more than 1 s. Harmless for the result. |
| `orders_not_returned` | Some requested orders were not returned by the node. | `order.info` | `data.not_returned` is non-empty. The node gives no reason (unknown id, not yours, expired...). |
| `invalid_order_timestamp` | One or more order timestamps cannot be shown as a UTC date. | `order.info`, `order.new`, `order.take-*`, `order.add-invoice`, `order.sync` | An `OrderRecord` `created_at`/`expires_at` is `null` for being out of range. |
| `local_status_stale` | The node accepted the invoice but the local order status could not be updated. | `order.add-invoice`, `order.fiat-sent`, `order.release`, `order.cancel` | The node moved on; the local row still shows the old status (`meta.persistence: failed`). The step itself **succeeded**; do not resend it. The message mentions "invoice" for every command. |
| `trade_keys_limited` | Trade keys older than the newest 200 were not read. | `message.list`, `order.sync` | You have more than 200 trade keys; the oldest were skipped (`data.keys_not_read`). Use `message list --order ID` for an old order. |
| `trade_index_not_recorded` | The request was accepted but its new trade index could not be recorded, so the next order may reuse it. | `order.fiat-sent`, `order.release`, `order.cancel` | Range maker only: the step succeeded, but the next-trade index it sent could not be saved locally. Two consequences: a later `order new` may reuse an index the node already saw and be refused, and `order sync` (which reads keys only up to the recorded last index) will not see the child order announced to that key. Repair with the human command `mostro-cli getlasttradeindex`, which asks the node for your last index and writes it to the database, then run `order sync`. |
