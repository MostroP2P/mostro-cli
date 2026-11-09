# Implementation Plan: Order Chat Without Looking Up The Counterparty

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-08 | **Status**: Units 1 to 4 implemented (the lookup, the chat transport, outcome types and renderer, the command in human mode, its JSON
output).

## Stack

Four stacked units, each a branch on the previous one; the first starts from `main` (8970c81). #246 (`order watch`) and #243 (next steps)
stack on top. Each unit stays within 1500 changed lines, generated fixtures and documents included.

| Unit | Content | User-visible |
| --- | --- | --- |
| 1 | `Store::counterparty`, `Store::set_counterparty`, `msg::counterparty`, `commands::order::counterparty`, `counterparty_unknown`, this folder | No |
| 2 | `msg/chat.rs` (read and send), `outcome/chat.rs`, `Stages::Chat`, `parse_human::table::escape_untrusted`, `parse_human/chat.rs` | No |
| 3 | `order chat` in human mode, registry `Unsupported`, discovery entry, `contracts/examples.json`, command reference, book | Yes |
| 4 | JSON on (`JsonSupport::Read`), the `--yes` rule, `docs/schemas/order-chat-v1.json`, regenerated discovery, book | Yes |

## Unit 1: the counterparty lookup

- `src/store/counterparty.rs`: `Store::counterparty(id)` opens the store read-only (no file created beside it) and returns `None` for NULL,
  empty or unparsable values and unknown orders; `Store::set_counterparty(id, key)` runs `UPDATE orders SET counterparty_pubkey = ?1 WHERE
  id = ?2` with canonical hex and fails (`Unwritable`) unless exactly one row changed. `store.rs` gains only `mod counterparty;`.
- `src/msg/counterparty.rs`: `counterparty_from_trade_pubkeys` moved verbatim from `parser/dms.rs` with its three tests (the legacy
  `persist_counterparty_pubkey` imports it; the frozen size of `parser/dms.rs` drops from 1210 to the measured 1189);
  `counterparty_in(mail, order, ours)` applies D10 to the mail `inbox::open_events` accepted, newest first, and only to mail addressed to
  `ours`.
- `src/commands/order/counterparty.rs`: `find_counterparty(order, trade, config, store, limits, now) -> Result<Counterparty,
  LookupFailure>`, both with private fields and getters. The stored key, if it parses and is not ours, is returned with empty warnings and
  no relay contacted (D10); otherwise `read_inbox` with that one key since 0, then `counterparty_in`, and a key found is written with its
  result ignored (D3). Success carries the read's warnings, failure the code and the read's warnings.
- Store error mapping (as `message list`): `StoreError::Missing` is `identity_missing`; `Unwritable` and `Unreadable` are
  `store_unreadable`. In unit 3 `Store::order_keys` returning `None` is `order_unknown`, before the lookup.
- `ErrorCode::CounterpartyUnknown`, exit 1, scoped to `order.chat`'s schema like every code added after the pilot.

## Unit 2: transport, outcome types, renderer

Described in [architecture](../../docs/architecture.md) and the module rustdoc (`msg/chat.rs`, `outcome/chat.rs`, `Stages::Chat`,
`parse_human/chat.rs`). Two notes: after one relay confirms, `send_chat` lets the others answer until the same deadline
(`Publishing::settle`, so a relay still authenticating is not cut off), so a confirmed send can take the whole read budget when another
relay never answers, within the 15 s already counted under Timing; and the human truncation line reads "Older messages may be missing:
the read reached its limit of 200", which covers both causes (a relay's limit and the cap).

## Timing

Default budgets (10 s connect, 15 s read, 1 s cleanup): the lookup contacts no relay when the row holds the key and takes up to 26 s
when it reads the node's mail; the chat read 26 s; the send 10 s connect, 15 s for a relay to confirm, 1 s cleanup. Worst case 52 s for a read, 78 s with `--message`; the book tells wrappers to allow 90 s.

## Unit 3: where the dispatch arm goes

`cli/migrated.rs` is at 393 production lines at the base; unit 3 re-measures it at its start:

- at most 395: one arm guarded by `if output == OutputMode::Human` (+5 lines);
- 396 or 397: an unguarded 3-line arm, with the human-only guard in `cli/order_chat.rs` answering `output_not_supported` through `cli.rs`'s
  rejection before any effect;
- above 397: first move `request_options` (about 29 production lines, used only by `migrated.rs` and `write.rs`) unchanged into
  `cli/write.rs`, then take the first path.

## Risks

- A stored key is used as it is (D10). The legacy `getdm` stores the first node `Order` payload naming both trade keys and never refreshes
  it. On Lightning that is always a post-lock message (the pre-lock payloads carry no trade keys), but a Cashu node's `waiting-seller-to-pay`
  to the seller names both before the lock, so a seller who ran `getdm` then keeps a taker who later left; mostro-cli cannot fund a Cashu
  escrow. Checking every stored key against the mail would cost a relay read on every call and is out of the minimal scope; units 3 and 4
  say so in the command reference and the book, with the `getdmuser -p` fallback.
- `read_inbox` asks each relay for its newest 50 messages, and the node's kind-14 messages expire on relays that honor NIP-40 after its
  `dm_days` (30 by default). A trade whose lock-time mail is beyond either and whose key was never stored gives `counterparty_unknown`,
  although the escrow was locked; units 3 and 4 say so in the command reference and the book, with the `getdmuser -p` fallback.
- Relays that require proof of work on kind 14 refuse the unmined chat envelope (`request_rejected`); `--pow` cannot help. A relay that
  asks for authentication gets an answer signed with a throwaway key (`ephemeral_client`), so one that accepts only known keys refuses
  it too.
- `--message` is on argv, visible in the process list; a stdin form is out of scope.
