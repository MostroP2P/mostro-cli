# Implementation Plan: Message List

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-01 | **Status**: Specified, not implemented.

## Summary

A read of the user's own mailbox: derive the trade keys, ask the relays once for the node's messages to them, decrypt, summarize each message into a typed record, and return them
with the read evidence. It reuses the bounded relay read and `Snapshot` of `order list`, the identity loader, `OrderRecord`, the envelope and the schema generator; the new parts are
the key derivation in `Store`, the message record and the summary of the payload variants. Evidence and decisions: [research.md](research.md).

## Technical Context

Rust 2021, MSRV 1.94, no new dependency. Test side: the loopback relay, a **mailbox** fixture that publishes pre-made node messages to chosen trade keys (wrapped as the node does) and
the process harness. Files at or below 400 production lines; imports at the top; tests before code; no live node, public relay or regtest without authorization.

## Design

1. **Store.** `Store::trade_keys(newest)` returns `(index, Keys)` for the newest indexes up to the recorded last index and the count not returned; read-only. For `--order`, the
   stored key (`Store::order_keys`) is read and its index is found among the derived keys, or the record's `trade_index` is null.
2. **Read.** `msg::inbox::read(config, keys, since, limits)`: one filter (kind 14, author the node, the keys as `p`, `since`, a per-relay limit) through `fetch_msg`; decrypt each event with
   the key it names; dedupe by event id; sort; cap. A relay that refuses many keys makes the read fail as a relay failure (the probe decides whether batching is needed). The relay limit of the filter is the 50 of `order list`'s snapshot, shared by all keys, so `truncated` is set whenever 50 or more events were received.
3. **Records.** `outcome/messages.rs`: `MessageRecord`, `MessagePayload`, `NextStep`, `MessageListData`, the mapping from `Message` and a `Payload` summary (typed for the flow's variants,
   `other` by name for the rest); `MessageListInput::validate`.
4. **Operation.** `commands/message/list.rs`: `message_list(input, config, identity_store, now, limits)` returning the typed outcome; no printing.
5. **Dispatch and grammar.** `Commands::Message { command: MessageCommands::List(ListArgs) }`; the registry entry `["message","list"]` (`JsonSupport::Read`) replaces `getdm`; `cli/message.rs`;
   `parse_human/message.rs`; legacy `Commands::GetDm`, `cli/get_dm.rs` and the `--from-user` branch removed.
6. **Discovery.** Command-scoped schema (read stages plus snapshot), `message-list-v1.json`, regenerated discovery.

## Risks

- **Compatibility**: scripts that ran `getdm` to make the CLI learn a counterparty key lose that side effect; documented, and the data carries the key.
- **Relay limits**: many `p` keys in one filter; per-relay limits can cut a busy mailbox, reported as `truncated`.
- **Window**: a message older than `--since` is not shown; the default is the legacy 30 minutes.
- **Untrusted text**: text payloads and node-chosen strings are escaped and bounded.
- **MCP**: tools that read direct messages stay broken until migrated.

## Complexity Tracking

| Addition | Why it is needed | Simpler option rejected |
| --- | --- | --- |
| One filter with many keys | N sequential reads would multiply the deadline | the legacy loop of N fetches |
| Index-based key derivation | a take whose save failed has no row but has a key | read keys from the orders table |
| Typed payload summary | agents need `next_step` and the invoices as data | print the raw JSON of the message |
