# Data Model: Bond Flows

No new output. The relevant fields of existing outcomes:

| Command | Case | Fields |
| --- | --- | --- |
| `order take-*`, `order new` | bonded node | `data.next_step` `pay_bond_invoice`, `data.payment_request` the bond invoice; the local row holds the order (status `pending`) and the trade key |
| `order cancel` | taker at `waiting-taker-bond`, maker at `waiting-maker-bond` | `data.result` `canceled`, local status `canceled` |
| `order cancel` | maker bond locked meanwhile | `node_refused`, reason `not_allowed_by_status` (source finding; not exercised by the scripted node) |
| `message list` | `add-bond-invoice` | `action`, `next_step` `add_bond_invoice`, `payload.kind` `bond_payout_request`, `payload.order.sats_amount` the share, `payload.slashed_at` |
| `message list` | `bond-invoice-accepted`, `bond-payout-completed`, `bond-slashed` | `action`, `next_step` null, `payload.kind` `order`, `payload.order` (amount: the share, or the slashed bond) |
