# Feature Specification: Message List For Humans And Agents

**Feature**: `006-message-list-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-01

**Status**: Specified; nothing implemented. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Sixth step of the AI-ready CLI roadmap: canonical `message list` (replacing `getdm`), a read-only command that returns what the node sent to the
user's trade keys, with human and JSON output. It closes the gap the earlier write commands left: after paying a bond, waiting for a seller or answering a
request, the next instruction arrives as a message, and only the human-only `getdm` could show it. Builds on
[add invoice](../005-invoice-json/spec.md), [order take](../004-order-take-json/spec.md), [order new](../003-order-new-json/spec.md),
[order info](../002-order-info-json/spec.md) and the [order-list pilot](../001-order-list-json/spec.md) (the read model, `Snapshot` evidence and the bounded
relay read); none of their documents change.

## Clarifications

### Session 2026-11-01

- Q: Which messages does it read? -> A: Messages the node sent to the user's trade keys (decided by the maintainer, 2026-11-01): what `getdm` shows by default. Peer chat
  (`getdm --from-user`) is text written by the counterparty and is out of scope; it leaves with `getdm` and returns in a later chat spec. `getdmuser` (one conversation
  by pubkey) is not touched. Messages whose author is not the configured node are ignored, never shown.
- Q: Does it write anything? -> A: No (decided 2026-11-01). The legacy `getdm` also saved the counterparty and solver pubkeys it learned from the messages. This command is
  read-only: it needs no writable store, never creates anything, and returns the counterparty pubkey as data when a message carries it. The commands that wait for
  a reply (`fiatsent`, `release`, ...) still persist what they see. Anything that relied on running `getdm` first to learn the counterparty key must pass it
  explicitly or wait for the spec that migrates it (see Risks in the plan).
- Q: Window and filter? -> A: `--since MINUTES` (default 30, as `getdm`, at most 10080 = 7 days) and an optional `--order UUID` that reads only the key stored
  for that order (decided 2026-11-01). The result is bounded and says when it is not the whole story.
- Q: Defaults not asked, open to veto: no `--yes`, no prompt (it is a read); records are ordered oldest first as in `getdm`; `getdm` is removed with no alias, including
  `--from-user`; JSON and human describe the same records.
- Q: What does a record say to do next? -> A: Each record carries the wire `action` and, for the few actions that ask the user for something, a typed `next_step`
  (`pay_bond_invoice`, `add_invoice`, `pay_hold_invoice`, `add_bond_invoice`) so that an agent can continue the flow without parsing prose. The invoices the node sends
  (bond, hold, payout request) are data of the record, never logged.

## Scope

`message list` derives the user's trade keys from the existing identity, asks every configured relay for the messages addressed to them in the window, decrypts
those authored by the node and returns them as records, with the evidence of what was read. Out of scope: peer chat, admin messages (`getadmindm`), acting on a message,
marking messages read, any local write, MCP.

## Requirements

- **FR-001**: `message list [--since MINUTES] [--order UUID]`. `--since` is an integer from 1 to 10080 (default 30); `--order` is a UUID, canonicalised to lowercase. `getdm`,
  `-s`, `-f` and `--from-user` are removed with no alias. Invalid values are `invalid_arguments` before any effect. No `--yes` and no stdin.
- **FR-002**: The identity comes from the existing store, read-only; none means `identity_missing`, an unreadable one `identity_unreadable`. Nothing is created or written.
- **FR-003**: The keys read are the trade keys of indexes 1 to the recorded last trade index (so a take whose row was never saved is still read), newest first, at most 200
  (more is the warning `trade_keys_limited` with the count not read). With `--order`, only the key stored for that order; an unknown order is `order_unknown` and nothing is asked.
- **FR-004**: One bounded read of the configured relays through `msg` (a single filter: kind 14, author the node, all keys, `since`, a per-relay limit), the same completion
  rules as `order list` (every relay must reach EOSE, any relay failure, the deadline or the cap fails the whole read, nothing is retried) and the same `Snapshot` evidence.
- **FR-005**: Each event is decrypted with the key it is addressed to and parsed as a Mostro message; events that do not decrypt, parse or come from the node are skipped
  without a diagnostic that names them. Duplicates (the same event from two relays) appear once. At most 200 records are returned, oldest first; more is `truncated: true`.
- **FR-006**: A record has the event time, the order id, the request id, the wire action, the trade index and public key it was addressed to, the typed `next_step` when the
  action asks for one, and a typed payload summary (see the data model). Text from the node is untrusted and escaped in human output. Invoices appear in the record and
  on stdout only.
- **FR-007**: One versioned JSON outcome with canonical name `message.list` (already in the registry), a command-scoped schema, fixtures and examples, regenerated discovery,
  drift, boundary and CI checks. No key, mnemonic or database record is serialized. Human and JSON describe the same records.
- **FR-008**: Statuses `ok` and `error` only (a read has no `uncertain`); exit 0, 2 for `invalid_arguments`, 1 otherwise. Codes: the read-command set of `order list` plus
  `identity_missing`, `identity_unreadable`, `order_unknown`, `store_unreadable`. An empty window is `ok` with no records, not an error.
- **FR-009**: All reception goes through `msg`; no new direct fetch or subscribe appears elsewhere. Diagnostics follow the allow-list and never include keys, relays,
  arguments, invoices or message text.
- **FR-010**: The legacy `getdm`, `cli/get_dm.rs` and its `--from-user` branch are removed with no alias after the new command is verified; the unmigrated commands keep
  rejecting JSON early.

## Success Criteria

- **SC-001**: Against the scripted node, a mailbox of messages (bond request, add-invoice, pay-invoice, waiting, cant-do, bond payout request, a text, an unknown action)
  yields one valid document whose records match what was sent, oldest first, with the right `next_step`.
- **SC-002**: Messages from another author, for a key that is not the user's, undecryptable, malformed or older than the window never appear; duplicates appear once.
- **SC-003**: Every failure leaves the store byte-identical; the command never writes, even on success.
- **SC-004**: `--order` reads only that order's key; the limits (keys, records, window) hold and are reported.
- **SC-005**: Human and JSON agree on every record; no secret sentinel and no invoice appears in a diagnostic stream; Linux and the CI-hosted Windows runner pass.
- **SC-006**: Nothing about a real node is claimed until a separately authorized probe is recorded.

## Assumptions And Open Items For The Plan

- A message addressed to a trade key is a kind-14 event authored by the node and tagged `p` with that key; `unwrap_incoming` decrypts it with that key. The scripted node
  needs a way to publish such messages unprompted (a mailbox), built with the same wrapping the real node uses.
- Whether the real relays honor one filter with many `p` keys and what limit they apply needs the probe; the plan splits the keys into batches if a relay refuses.
- `Payload` has many variants. The record types the few the flow needs and reports the others by name only (`other:<variant>`), with no content.
- The legacy default of 30 minutes can miss a message in a slow flow; the caller passes `--since`. A cursor is a later improvement.
- Windows follows CI; the probe needs separate authorization.
