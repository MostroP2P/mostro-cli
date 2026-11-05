# Data Model: Range Order Children

## `order.sync` data

| Field | Meaning |
| --- | --- |
| `node_pubkey` | the node's public key (hex) |
| `since_minutes` | the window read, a decimal string |
| `adopted` | the orders saved by this run, oldest first: `order` (the order record), `trade_index` (decimal string), `trade_pubkey` (hex) |
| `messages_read` | the messages of the node read in the window, a decimal string |
| `messages_ignored` | those that led to no row (other actions, known orders), a decimal string |
| `truncated` | a relay reached its per-relay limit or more than the record cap matched |
| `keys_read`, `keys_not_read` | the trade keys read and the older ones left out (decimal strings) |

`meta.publication` and `meta.daemon_acceptance` are `not_applicable`; `meta.snapshot` describes the relay read. `meta.persistence` is `applied`
(a row was saved), `not_attempted` (nothing was due, or the run stopped after reading and before saving) or `failed` (a save failed after others
succeeded: `uncertain` / `order_not_saved`, with `adopted` listing what was saved). Before any relay is read it is `not_applicable`.

## Which announcements count

A `new-order` message the node signed, addressed to one of the user's derived keys, in the window, whose payload order has the message's own id,
a kind and a status, announced to a key whose index is known, and whose id the store does not have. The newest announcement of an id wins. Anything
else (other actions, other authors, other recipients, mismatched ids, no order, no index) is ignored and counted in `messages_ignored`.

## Trade steps (amends spec 007)

| Field | Meaning |
| --- | --- |
| `next_trade_index` | the index reserved for the next trade key a range order's maker step carried; `null` otherwise |
| `next_step` | `sync_child_order` after a `release` that sent a key; `wait_for_release` after a buyer's `fiat-sent`, whether or not it sent a key |

The key is sent only for a range order the user made, with room left (`max - fiat_amount >= min` on the local row), at the maker's own step.
The index is recorded in the store before publishing (the reservation is the spend). A request that never left, or that the node refused, gives
it back unless another command reserved one meanwhile. An index that cannot be reserved stops the step with `store_unwritable` before anything is
sent (amended 2026-11-05 by spec 008, slice 7; the warning `trade_index_not_recorded` is removed).
