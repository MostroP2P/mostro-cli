# Data Model: Next Steps Of Message Records

Extends [spec 006](../006-message-list-json/data-model.md). Envelope, stages and codes are unchanged.

## `next_step`

| Value | Action | Since |
| --- | --- | --- |
| `pay_bond_invoice` | `pay-bond-invoice` | spec 006 |
| `add_invoice` | `add-invoice`; `payment-failed` with `payment_attempts` `"0"` | spec 006; the payment-failed case unit 1 |
| `pay_hold_invoice` | `pay-invoice` | spec 006 |
| `add_bond_invoice` | `add-bond-invoice` | spec 015 |
| `fiat_sent` | `hold-invoice-payment-accepted` (buyer only) | unit 2 |
| `release` | the seller's `fiat-sent-ok` (stored seller row, same key, no request id) | unit 3 (proposed) |
| `rate` | `rate` (both sides, after the end of a completed trade) | unit 2 |

Every other message is `null`. The enum is open within v1: a consumer reads an unknown value as `null`.

## Payload `payment_failed` (unit 1)

| Field | Meaning |
| --- | --- |
| `kind` | `payment_failed` |
| `payment_attempts` | how many more times the node will try to pay, as of this message (mostrod v0.20.2: its setting minus one), decimal string |
| `payment_retries_interval` | seconds between two attempts, decimal string |

Decimal strings because every number in `message.list` data is one. Releases up to 0.17.0 reported the same message
as `{"kind":"other","variant":"payment_failed"}`; the change comes after 0.17.0 and is not yet released. The node sends it once per
buyer invoice: on the first failed payout to it, and again after a replacement invoice resets the count, never updated in between. The payload kind is open within v1: a consumer reads only the `kind` of a kind it does not
know, and the `variant` of `other` is informative only.

## The user's side (unit 3, proposed)

| `orders.kind` | `orders.is_mine` | Side |
| --- | --- | --- |
| `sell` | true (made) | seller |
| `buy` | false (taken) | seller |
| `sell` | false (taken) | buyer |
| `buy` | true (made) | buyer |

`is_mine` NULL, an unknown kind, no row or another trade key: no side, so no `release`. Rows written by 0.16.3 and earlier say "made"
for every order; the request-id condition keeps `release` off such a buyer. `restore` writes no rows, and a range child has none
before `order sync`.
