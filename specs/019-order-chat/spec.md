# Feature Specification: Order Chat Without Looking Up The Counterparty

**Feature**: `019-order-chat`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-08

**Status**: Implemented (units 1 to 4): `order chat` in human mode and `--output json`. Verified on Linux against isolated stores,
loopback relays and the scripted node's mail only; not probed on a real node. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Issue #245. An agent that ran a full trade with mostro-cli 0.17.0 had to find the counterparty's trade key itself and pass it to
`getdmuser -p` and `dmtouser -p`, which print human text only. On the JSON flow nothing states that key: `message list` carries it inside
`buyer-took-order` and `hold-invoice-payment-accepted` (an `Order` payload naming both trade keys) and `fiat-sent-ok` (a `Peer` payload), and
the store's `counterparty_pubkey` column is filled only by the legacy `getdm`. Proposed: a migrated `order chat --order <uuid>` that finds the
counterparty on its own, reads the conversation and, with `--message`, sends one message. #246 (`order watch`) reuses the lookup.

## Clarifications

### Session 2026-11-08 (the design; D2, D3, D6 and D8 are flagged for the user's confirmation)

- **D1 `--since`**: minutes, 1 to 10080, default 10080 (the seven-day chat window); anything else is `invalid_arguments`.
- **D2 Read before send (flagged)**: with `--message` the order is lookup, chat read, publish, so a failed read publishes nothing. The issue
  says "sends, then reads": sending first would leave an error with null data after a failed read and lose the id of the message just sent.
  The trade-off: a relay that fails the read also blocks a send, which is safe to retry because nothing was sent.
- **D3 Persistence (flagged)**: `persistence` is always `not_applicable`. A key learned from the node is written to the row as a best-effort
  cache, one `UPDATE` after the read with no transaction held; a failed save changes nothing in the outcome and the next run looks again.
- **D4 Stages (an amendment for order chat)**: registry `JsonSupport::Read`. Publication is `not_applicable` until the send step runs, with
  or without `--message`: Clap rejections, the missing `--yes`, arguments, configuration, identity, store, `order_unknown`,
  `counterparty_unknown`, and lookup or read failures. The send step starts at the wrap and reports `published`; `not_attempted` when the wrap
  fails (`internal_error`), the relays cannot be connected (the connect code, `relay_connection_failed` or `internal_error`) or every relay
  refused (`request_rejected`); `unknown` (`uncertain`, `request_rejected`) when no relay confirmed and one failed ambiguously. This amends
  the general meanings ("publishes nothing", "stopped before"): the registry reports a Clap rejection of a `--message` call as a read before
  `--message` is known, so one rule is the only one that holds for every call. It matches `order sync`. Combinations: `[na,na,na]` and
  `[published,na,na]` (ok), `[na,na,na]` and `[not_attempted,na,na]` (error), `[unknown,na,na]` (uncertain).
- **D5 `--message`**: one value, trimmed, 1 to 4096 bytes after trimming (so a message we send is never cut when read back); a second
  `--message` is a Clap error; a text starting with `-` uses `--message=<text>`.
- **D6 Consent (flagged)**: JSON with `--message` and without `--yes` is `invalid_arguments` (exit 2) before any configuration, store or
  relay access. Human mode sends without a prompt, like `dmtouser`. `--yes` without `--message` is accepted and does nothing.
- **D7 Settings that do not apply**: `--pow`/`POW`, `--transport`/`TRANSPORT` and `--reply-timeout`. The chat envelope goes to relays,
  never to the node; mostro-core's chat wrap mines no work; nothing waits for a reply.
- **D8 `sent` (flagged)**: the sent message as a chat record, non-null exactly when publication is `published` or `unknown`; `messages`
  never contains it. The issue asks for "the same shape the other writes use": they report publication in `meta.publication` and their
  result in `data`, so here `meta.publication` carries the publication and `data.sent` the message.
- **D9 Names and schema files (a recorded departure)**: canonical name `order.chat`. `chat.history` (`getdmuser`) and `chat.send`
  (`dmtouser`) stay, unchanged and human-only, as the fallback when `order chat` answers `counterparty_unknown` and the caller knows the
  counterparty's key; like `order chat`, both need the order's row and its trade key. The issue asks for input and result schemas under
  `docs/schemas/`; by the repository's convention the input schema is generated into the discovery entry only, and only the output schema gets
  its own file, `docs/schemas/order-chat-v1.json`. The book still gets `schemas/order.chat.input.json`.
- **D10 Counterparty sources**: in the issue's order. A stored `counterparty_pubkey` that parses and is not our trade key is used as it
  is, with no relay contacted. Otherwise all the node's mail to the order's trade key since 0 is read, and only (a) `buyer-took-order` or
  `hold-invoice-payment-accepted` with an `Order` payload whose id (or the message's) is the order, naming both trade keys with one of them
  ours, or (b) `fiat-sent-ok` with a `Peer` payload whose message id is the order and whose key parses and is not ours, name the
  counterparty. The newest such message wins and is cached in the row (D3). `waiting-seller-to-pay`, a `new-order` republication, the
  solver of `admin-took-dispute` and chat messages never do. A failed read fails the lookup. A stored key is not checked against the mail:
  the legacy `getdm` stores the first node `Order` payload naming both keys and never refreshes it, which on Lightning is always a post-lock
  message, but a Cashu node's `waiting-seller-to-pay` to the seller names both before the lock (plan Risks). An old trade whose key was
  never stored and whose lock-time mail is gone is `counterparty_unknown`; `getdmuser -p` stays the fallback.
- **D11 Rollout**: unit 3 ships the command in human mode (`--output json` is `output_not_supported`); unit 4 turns JSON on.
  `meta.snapshot` is always null; `truncated` is the evidence of the bounded read.

## Requirements

- **FR-001**: `mostro-cli order chat --order <uuid> [--message <text>] [--since <minutes>] [--yes]`, canonical `order.chat`, migrated; the
  caller never passes the counterparty's key.
- **FR-002**: The counterparty comes from the order's row or, when that holds no usable key, from the node's post-lock mail, as in D10; a
  key found in the mail is cached in the row (D3). Neither is `counterparty_unknown` (exit 1): "The counterparty of this order was not found in the node's messages; it is known
  once the escrow is locked."
- **FR-003**: The conversation is one bounded read of the kind-14 chat of the order's trade key and the counterparty: both sides, oldest
  first, at most 200, opened only through `unwrap_chat_message` with our key and the counterparty's as the allowed signers.
- **FR-004**: `--message` publishes one chat message after the read (D2, D5, D6); a send no relay confirms is `uncertain` and never retried.
- **FR-005**: Untrusted text is bounded to 4096 bytes and escaped in human output (control, Unicode format and line separator characters;
  the zero-width non-joiner and joiner are kept). The message text never appears in errors, verbose output or stderr; the trade secret
  is never serialized.
- **FR-006**: Output schema `docs/schemas/order-chat-v1.json`, discovery, examples and drift tests (D9); `schema_version` stays `1`.

## Success Criteria

- **SC-001**: With a loopback relay holding the node's `hold-invoice-payment-accepted` from eight days ago, the lookup returns the seller's
  trade key and saves it; the next lookup uses the row. A stored key is used with no relay contacted.
- **SC-002**: With an empty row and only pre-lock mail (`waiting-seller-to-pay` naming another key, `new-order`) the lookup is `counterparty_unknown` and the
  store does not change; a forged `Order` payload from another author never names the counterparty.
- **SC-003**: `order chat` reads both sides oldest first and sends one message the counterparty opens; refused and unconfirmed sends are
  `error` and `uncertain` with the D4 stages.
- **SC-004**: JSON `--message` without `--yes` exits 2 before any relay connection, with the store unchanged.
