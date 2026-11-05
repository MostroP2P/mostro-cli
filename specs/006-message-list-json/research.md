# Research: Message List

Read 2026-11-01 in this repository: `src/cli/get_dm.rs`, `src/util/events.rs` (`fetch_events_list`), `src/parser/dms.rs` (`parse_dm_events`, the display
and persistence of each action), `src/msg/receive_msg.rs` (`fetch_msg`, `Snapshot`), `src/msg/orders.rs`, the daemon's `enqueue_order_msg` and the probe of
2026-11-01 (spec 005).

## What `getdm` does

- Derives, for every index from 1 to the context's trade index, the trade key from the mnemonic (`User::get_trade_keys`), and for each key runs **one fetch** of kind 14
  authored by the node and tagged with that key, with a seven-day window and a 15 s timeout per fetch; so N keys are N sequential reads on one client.
- Decrypts with `unwrap_incoming(event, key)` (kind 14, NIP-44 direct), keeps those newer than `--since` minutes, sorts by time, prints a table.
- Then, for every message from the node, saves the counterparty pubkey of a `Peer` payload and a solver pubkey (a local write), and for a bond payout request reads the
  node's claim window from its info event.
- `--from-user` switches to peer chat: the same loop but with NIP-17 style decryption of text written by another user.

## What the probe showed (spec 005)

After a take on a bonded node the taker received, in order: `pay-bond-invoice` (the bond invoice and the order), then, 2 minutes after the bond was paid, `add-invoice`
(the order, 6233 sats). Without a way to read them, the flow stalled for a JSON user.

## Consequences

- A single filter with all the `p` keys is one subscription instead of N; the read reuses `fetch_msg`, its caps and `Snapshot`, and matches `order list`'s completion rules.
- Keys are derived by index, not read from the orders table: a take whose save failed (`uncertain`) still has a key, and its messages are the way to find out.
- The node's claim window is only needed to print a deadline for a bond payout request; the record carries the request's own fields and the caller can compute it. No extra read.
- The legacy saves are the only writes. Dropping them makes the command a pure read; the cost is documented in the spec.
- The scripted node answers requests; it does not volunteer messages. The fixture needs a mailbox that publishes pre-made messages to chosen trade keys.
