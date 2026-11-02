# bond add-invoice

| | |
| --- | --- |
| `command` | `bond.add-invoice` |
| Kind | Write |
| Needs | `MOSTRO_PUBKEY`, `RELAYS`, identity, the order's local row, `--yes` in JSON mode |
| Publishes | One request with the payout invoice, signed with the order's trade key |
| Local write | None (`persistence` is `not_attempted` on success) |
| Statuses | `ok`, `error`, `uncertain` |
| Schemas | `schemas/bond.add-invoice.input.json`, `schemas/bond.add-invoice.output.json` |

On a bonded node, when the **counterparty's** bond was slashed, part of it is owed to you.
The node asks for an invoice (`message list` shows `next_step: add_bond_invoice` with a
`bond_payout_request` payload); this command sends it.

## Invocation

```text
mostro-cli --output json bond add-invoice --order UUID --invoice BOLT11 --yes
```

Replaces `addbondinvoice`.

## Input

| Option | Required | Rule |
| --- | --- | --- |
| `--order` | yes | UUID of the order whose bond was slashed |
| `--invoice` | yes | A BOLT-11 invoice for **exactly your share** of the slashed bond |
| `--yes` | **yes in JSON** | |

## Success data: `BondInvoiceData`

| Field | Type | Meaning |
| --- | --- | --- |
| `node_pubkey` | hex64 | The node |
| `order_id` | uuid | The order |
| `result` | `"invoice_accepted"` | The node accepted the invoice and pays it from its wallet |
| `counterparty_share_sats` | decimal string or null | The share amount the node reported; null when it reported none |
| `trade_pubkey` | hex64 | The order's trade key |

## Outcomes

| Case | status | code | Stages |
| --- | --- | --- | --- |
| Accepted | `ok` | | published / accepted / not_attempted |
| Local failure, `order_unknown` | `error` | its code | not_attempted / unknown / not_attempted |
| Refused (wrong amount...) | `error` | `node_refused` + `reason` | published / rejected / not_attempted |
| No / unusable reply | `uncertain` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` | published / unknown / not_attempted |
| Ambiguous relays | `uncertain` | `request_rejected` | unknown / unknown / not_attempted |

After `uncertain`, wait for the payout (or a new request in `message list`) before sending
another invoice.

## Errors

`invalid_arguments`, `configuration_*`, `relay_connection_failed`, `relay_read_failed`,
`internal_error`, `identity_*`, `protocol_unsupported`, `pow_required`, `request_rejected`,
`node_refused`, `order_declined`, `order_unknown`; uncertain: `reply_timeout`,
`reply_malformed`, `request_rejected`, `relay_read_failed`, `pow_required`.

## Warnings

`invalid_event_timestamp`, `cleanup_incomplete`.

## Examples

{{#include ../generated/bond_add_invoice.md}}

## Next

[Bonds](../flows/bonds.md)
