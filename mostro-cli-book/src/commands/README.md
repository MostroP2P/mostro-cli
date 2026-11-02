# Command index

Every JSON-capable command, with what it touches. "Identity" means the local `mcli.db`
must already exist; no JSON command creates it.

| CLI path | `command` | Kind | Network | Identity | Local write | `--yes` | Statuses |
| --- | --- | --- | --- | --- | --- | --- | --- |
| [`schema`](schema.md) | `schema` | offline | none | no | no | no | ok, error |
| [`order list`](order_list.md) | `order.list` | public read | reads relays | no | no | no | ok, error |
| [`order info`](order_info.md) | `order.info` | request | 1 signed request | yes (read) | no | no | ok, error |
| [`message list`](message_list.md) | `message.list` | private read | reads relays | yes (read) | no | no | ok, error |
| [`order sync`](order_sync.md) | `order.sync` | private read + local write | reads relays | yes | order rows | no | ok, error, uncertain |
| [`order new`](order_new.md) | `order.new` | write | 1 signed request | yes | index + row | **yes** | ok, error, uncertain |
| [`order take-sell`](order_take_sell.md) | `order.take-sell` | write | 1 signed request | yes | index + row | **yes** | ok, error, uncertain |
| [`order take-buy`](order_take_buy.md) | `order.take-buy` | write | 1 signed request | yes | index + row | **yes** | ok, error, uncertain |
| [`order add-invoice`](order_add_invoice.md) | `order.add-invoice` | write | 1 signed request | yes | status | **yes** | ok, error, uncertain |
| [`bond add-invoice`](bond_add_invoice.md) | `bond.add-invoice` | write | 1 signed request | yes | none | **yes** | ok, error, uncertain |
| [`order fiat-sent`](order_fiat_sent.md) | `order.fiat-sent` | write | 1 signed request | yes | status (+ index) | **yes** | ok, error, uncertain |
| [`order release`](order_release.md) | `order.release` | write | 1 signed request | yes | status (+ index) | **yes** | ok, error, uncertain |
| [`order cancel`](order_cancel.md) | `order.cancel` | write | 1 signed request | yes | status | **yes** | ok, error, uncertain |

All of them need `MOSTRO_PUBKEY` and `RELAYS` except `schema`, which needs nothing.

## Which key signs what

| Command | Signed with |
| --- | --- |
| `order info` | Identity key (index 0) |
| `order new`, `order take-*` | A **new** trade key at the next trade index (reserved locally first) |
| `order add-invoice`, `bond add-invoice`, `order fiat-sent`, `order release`, `order cancel` | The trade key stored with the order's local row (hence `order_unknown` when the row is missing) |

## Command page layout

Each page has the same sections: a summary table, **Invocation**, **Input**, **Behavior**,
**Success data**, **Outcomes**, **Errors**, **Warnings**, **Examples** and **Next**.
The input and output schemas quoted are in `schemas/<command>.input.json` and
`schemas/<command>.output.json`.

## Input schemas

The discovery document gives an `input_schema` per command, generated from the argument
parser: one property per long option (`fiat-amount`, `yes`...). All values are typed as
strings because they are command-line tokens; boolean flags appear as `"true"`/`"false"`.
A wrapper can use it to build forms or LLM tool definitions, then turn the object into
`argv`: `{"order": "…", "yes": "true"}` → `--order … --yes`. Repeatable options
(`order info --order`) are given once per value.
