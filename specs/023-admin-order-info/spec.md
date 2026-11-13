# Feature Specification: `admin order-info`

**Feature**: `023-admin-order-info`

**Working Branch**: `feat/admin-order-info`, from `main` (feature directory naming does not change Git branches)

**Created**: 2026-11-12

**Status**: Implemented. FR-001 to FR-012 are verified on Linux against the scripted node only; SC-006 (a real node on regtest) was not
run, and the `mostro-core` pin by git revision is still in place. Progress is tracked only in [tasks.md](tasks.md).

**Input**: [MostroP2P/mostro#1107](https://github.com/MostroP2P/mostro/issues/1107). A solver or the admin of a node has no way to
learn the real state of an order: the public order event only says `pending`, `in-progress`, `success` or `canceled`, and
`order info` only answers for the caller's own orders. The node now answers the action `admin-order-info` with one snapshot of any
order, from its database and from its Lightning node. This command asks for it.

## Dependencies

- `mostro-core` with `Action::AdminOrderInfo` and `Payload::OrderInfo`
  ([MostroP2P/mostro-core#190](https://github.com/MostroP2P/mostro-core/pull/190)). Until it is released the CLI pins that pull
  request by git revision; the pin is replaced by the released version before this is merged.
- A node that answers the action ([MostroP2P/mostro#1110](https://github.com/MostroP2P/mostro/pull/1110)). An older node drops the
  request and the command ends with `reply_timeout`.

## What The Node Answers (source reading, mostro `src/app/admin_order_info.rs`)

- The request goes in the `order` wrapper: `id` is the order id, no payload. The reply has the same `id` and `request_id` and an
  `order_info` payload, sent to the key that signed the request.
- Allowed for the node's own key and for any registered solver, `read` or `read-write`, on any order. The check runs against the
  identity key. Anyone else gets `cant-do` `invalid_pubkey`; an unknown order gets `not_found`.
- The order part comes from the database and is always there. `hold_invoice` and `payout` come from the Lightning node and are
  `null` when the order has no hash to ask about. When there is one, the state is what the node reports, `not-found` (the node has
  no record of the hash) or `unavailable` (it could not be asked: an error, or no answer in 2 seconds).
- `payout` is present only while a payment to the buyer is in flight. After a failed payout it is `null`, and `failed_payment` and
  `payment_attempts` tell what happened.
- `bonds` holds the maker bond with its range child rows, the bond of the current taker, and any row still owed a payout.
- It never carries a preimage, an invoice, a Cashu token, a payment hash or a master pubkey.

## Decisions

- **D1, the command.** `mostro-cli admin order-info --order UUID`, canonical name `admin.order-info`, JSON support `Request`: it
  publishes one request and writes nothing locally. `--order` is required and takes one id: the action answers for one order. `admin`
  is a new command group; the existing admin commands (`admcancel`, `admsettle`, `admtakedispute`, `admaddsolver`) stay as they are
  and are not migrated here.
- **D2, the key.** The request is signed with the key in the environment variable `ADMIN_NSEC`, used as both the identity and the
  author, as the existing admin commands do. It is read from the environment only: the config file refuses it (spec 018). The local
  identity store is not opened, read or created. A variable that is missing or empty is `admin_key_missing`; one that is not a
  private key is `admin_key_invalid`. Both fail before any relay is contacted. The key is never printed, logged or serialized.
- **D3, one exchange.** The request goes through `msg::exchange` as `order info` does: node-info probe, the advertised proof of
  work mined up to 20 bits, one publication, the reply matched on the request id, no retry. The lane is `FirstContact`: the node
  knows a solver's key only while it holds an active dispute, and work mined for the first-contact lane is accepted on either.
- **D4, the reply.** Accepted actions are `admin-order-info` and `cant-do`. A reply whose order id is not the requested one, or
  whose payload is not an `order_info`, is `reply_malformed`. When `ADMIN_NSEC` is the node's own key the request itself matches
  the reply filter; `msg::reply` already skips our own event.
- **D5, refusals stay refusals.** `invalid_pubkey` and `not_found` are `node_refused` with the node's reason. Unlike `order info`,
  `not_found` is not turned into an empty success: one order was asked for and the node says it does not exist.
- **D6, the data.** `data` is a record of this CLI, mapped field by field from the payload; no core object is serialized
  wholesale. Fields shared with `order.info` keep its names and formats (`side`, `sats_amount`, `fiat_currency`,
  `premium_percent`, decimal strings for amounts, each time as RFC 3339 plus `_unix`). The fields are in the
  [contract](contracts/cli.md#data).
- **D7, open enums.** `status`, `side`, the hold invoice and payout states, the failure reason and the bond role, state, slash
  reason and kind are strings in the node's spelling, not closed enums in the output schema. The command exists to show what the
  node holds, including a status or state added after this CLI was built; a closed enum would turn that into a failure. The known
  values are documented, and `unknown` (the fallback of `mostro-core` for a value it does not know) is passed through.
- **D8, untrusted text.** `payment_method`, `fiat_currency` and `mint_url` come from the node. Human mode shows them with
  `escape_controls`, like `order list`.
- **D9, human output.** Four blocks, each printed only when it has something to show: the order, the escrow (hold invoice or Cashu
  escrow), the payout, and the bonds. A part the node could not ask about says so (`unavailable`) instead of being left out.

## User Scenarios & Testing

### Story 1: a solver looks at a stuck order (P1)

**Given** a registered solver's key in `ADMIN_NSEC` and an order in `settled-hold-invoice` whose payout failed, **when** the solver
runs `admin order-info --order ID --output json`, **then** the outcome is `ok` with `status` `settled-hold-invoice`,
`hold_invoice.state` `settled`, `payout` `null`, `failed_payment` `true` and the payment attempts.

### Story 2: the Lightning node is down (P1)

**Given** the same solver and an order with a hold invoice, on a node whose Lightning node does not answer, **when** the solver
asks, **then** the outcome is still `ok`, with the order fields and `hold_invoice.state` `unavailable`.

### Story 3: a key that may not ask (P1)

**Given** a key in `ADMIN_NSEC` that is neither the node's nor a solver's, **when** it asks for any order, **then** the outcome is
the error `node_refused` with reason `invalid_pubkey`, exit 1.

### Story 4: no admin key (P1)

**Given** no `ADMIN_NSEC`, **when** the command runs, **then** it fails with `admin_key_missing` before any relay is contacted, and
no identity store is created.

### Story 5: the admin is the node (P2)

**Given** the node's own key in `ADMIN_NSEC`, **when** it asks for an order, **then** the command returns the node's reply and not
its own request.

## Requirements

- **FR-001**: `admin order-info --order UUID` publishes one `admin-order-info` request in the `order` wrapper, with the order id in
  `id`, a request id and no payload, signed with the `ADMIN_NSEC` key as identity and author.
- **FR-002**: Local failures come before any network access, in this order: arguments, node configuration, the admin key.
- **FR-003**: The command opens no local store and creates no file or directory.
- **FR-004**: A reply `admin-order-info` with an `order_info` payload for the requested order is `ok`, with the
  [contract's data](contracts/cli.md#data). Every field is mapped explicitly.
- **FR-005**: A `cant-do` reply is `node_refused` with the node's reason; `daemon_acceptance` is `rejected`.
- **FR-006**: A matching reply with another payload, or for another order, is `reply_malformed`.
- **FR-007**: The failures of `msg::exchange` keep their codes: `protocol_unsupported`, `pow_required`, `request_rejected`,
  `reply_timeout`, the relay and configuration codes, `internal_error`.
- **FR-008**: `admin_key_missing` and `admin_key_invalid` are new error codes, exit 1, listed only in this command's schema.
- **FR-009**: `--output json` prints one document of schema version 1; `schema` lists `admin.order-info` with generated input and
  output schemas, and `docs/schemas/admin-order-info-v1.json` is checked in and drift-checked.
- **FR-010**: Human mode prints the D9 blocks on stdout and one error line on stderr for a failure, with the exit of the outcome.
- **FR-011**: The key in `ADMIN_NSEC` appears in no output, log or error message.
- **FR-012**: `docs/commands.md`, `docs/architecture.md` and `AGENTS.md` describe the command.

## Success Criteria

- **SC-001**: Against the scripted node, Story 1 to Story 5 produce the outcomes above, in JSON and in human mode.
- **SC-002**: Every JSON outcome of the process tests validates against `docs/schemas/admin-order-info-v1.json`.
- **SC-003**: With `ADMIN_NSEC` unset, the process exits 1 with `admin_key_missing`, the scripted relay records no connection and
  `MCLI_DIR` stays empty.
- **SC-004**: A reply carrying a state this CLI does not know (`"a-state-added-later"`) is `ok` and shows that text.
- **SC-005**: All constitution gates pass; the checked-in schemas match the generated ones.
- **SC-006** (manual, regtest): against a real node built from mostro#1110, a solver's query returns the order's stored status and
  the hold invoice state its Lightning node reports.

## Out Of Scope

- Migrating the other admin commands to typed outcomes.
- A gRPC equivalent: the node has no order query in its admin RPC.
- Asking for several orders in one command: the action takes one id.
- The MCP server and the mostro-cli book: a tool and a page for this command are follow-ups there.
