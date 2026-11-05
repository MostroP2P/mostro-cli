# Feature Specification: Order New For Humans And Agents

**Feature**: `003-order-new-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-10-29

**Status**: Implemented and verified on Linux and on the CI-hosted Windows runner against a scripted fake daemon; probed on regtest (mostrod v0.19.3, 2026-11-01) for the main paths; bonded and Cashu nodes are not covered. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Third step of the AI-ready CLI roadmap (critical-path step 3): canonical `order new`, replacing
`neworder`, with human and JSON output, an explicit confirmation rule, the `uncertain` status and a local save.
Builds on [order info](../002-order-info-json/spec.md) (identity loader, `msg::exchange`, request stages) and the
[order-list pilot](../001-order-list-json/spec.md); neither's documents change.

## Clarifications

### Amendment 2026-11-01: the maker bond is a next step, not an error

- The first implementation reported a bonded node as the error `bond_required` and dropped the bond invoice, so a CLI user could not pay it. Found by
  trying a take on a bonded node. Corrected: the `pay-bond-invoice` reply (parked order plus bond invoice) is accepted data. `order new` is `ok` with
  `data.next_step` `pay_bond_invoice` (otherwise `wait_for_taker`) and the bond invoice in `data.payment_request`; the parked order and the trade index are saved
  in one transaction. `bond_required` is removed. The command still pays nothing; the user pays the invoice with any Lightning wallet and the node then
  publishes the order and sends its `new-order` acknowledgement as a later message.

### Session 2026-10-29

- Q: With no local identity, should `order new` create one as the legacy `neworder` does? -> A: No (decided by the maintainer, 2026-10-29). It fails with `identity_missing` before any network access and creates nothing, as `order info` does. An agent never creates keys silently.
- Q: How do confirmation and the trade index behave? -> A: In JSON mode `--yes` is required and stdin is never read; without it the command fails with `invalid_arguments` (exit 2) and does nothing. Human mode keeps the preview and the prompt (blank or `y` accepts, `n` cancels, closed stdin is not approval). The trade index is reserved after every local check and the confirmation, right before the single publish, by recording it in the store in one short write transaction committed before anything is sent: the reservation is the spend. A failure before that point leaves the database byte-identical. A request that then never leaves the machine, or that the node refuses, gives the index back unless another command reserved one after it, in which case the index is skipped (harmless). A refusal for a stale index (`invalid_trade_index`) keeps it instead: the node is already at or past it, and since the node can record an index and still refuse (the first request of an identity it does not know, a take that lost the order), giving it back would make every later request carry it again (amended 2026-11-05 by spec 008, review of slice 7). The human preview shows the index the order will use unless another command reserves one meanwhile; the result always reports the index and key actually used (decided 2026-10-29, amended 2026-11-05 by spec 008).
- Q: What if the request was published but the outcome is unknown, or the node accepted and the local save failed? -> A: The roadmap's `uncertain` status is used (decided 2026-10-29): the node may have created a live order, so the outcome says so, carries the order id when the node returned one, is never retried by the CLI, and its documentation tells callers to check `order list` before trying again.
- Q: Keep the call to api.yadio.io that validates the currency when the amount is market-priced? -> A: No (decided 2026-10-29). A signed-order command makes no third-party HTTP call; the daemon validates and its refusal is reported as `node_refused` with the reason.
- Q: Does this spec cover `order take`? -> A: No. `order take` (both sides, take-with-invoice save outcome) is a later spec. No MCP change is part of this one; MCP `create_order` keeps failing against the CLI until migrated.

## Scope

`order new` publishes one signed `new-order` request with a freshly derived trade key and waits for the
correlated reply. On success the node's order is saved locally (the row the later commands, such as `order take`
and `addinvoice`, need) and returned as data. Nothing else is in scope: no take, no invoice flow, no range-order
follow-up, no MCP.

## Requirements

- **FR-001**: `order new` takes the side, currency, fiat amount (or a range), payment method, optional sats amount (default market price), premium, optional buyer invoice and optional expiration in days. The canonical long flags follow the roadmap (`--side buy|sell`, `--currency`, `--fiat-amount` with `--fiat-amount-max`, `--payment-method`, `--sats`, `--premium`, `--invoice`, `--expiration-days`, `--yes`); the plan fixes the exact grammar and which legacy spellings (`neworder`, `-k`, `-f 100-200`) are removed without alias. Invalid values are `invalid_arguments` before any effect.
- **FR-002**: Identity comes from the existing store, read-only first; no identity means `identity_missing` and nothing is created (no mnemonic, no directory, no database).
- **FR-003**: Confirmation: JSON mode requires `--yes`, never prompts and never reads stdin; human mode shows the order preview and asks. A declined or unconfirmed order is a stable outcome with nothing published and nothing saved.
- **FR-004**: Exactly one signed request is published, with a trade key derived at the next index, reserved right before publishing by recording the index in the store (one `BEGIN IMMEDIATE` transaction under the store's write lock, committed before anything is sent and never held across the network), so a run that dies after publishing never reuses the key and concurrent commands never share an index; the order is saved with it afterwards. Concurrent requests holding distinct indexes may reach the node out of order, and the node then refuses the lower one with `invalid_trade_index` (amended 2026-11-05 by spec 008). The identity key proves ownership as in the legacy flow. No retry, no second publication.
- **FR-005**: The reply is selected by correlation (request id, node author, addressed to the trade key, allowed actions `new-order` and refusal), armed before publishing, bounded by a deadline, released on every exit, reusing `msg::exchange`.
- **FR-006**: On the node's accepted reply the order is saved locally. If the save fails the outcome is `uncertain` (the node accepted, local state is missing) with a distinct code, the order id and the data needed to recover it; the plan decides whether the recovery data excludes key material.
- **FR-007**: The status is `ok` only when the node accepted and the save succeeded; `error` only when the request was provably not accepted or never published; `uncertain` when published and the result or the save is unknown. Stages (`publication`, `daemon_acceptance`, `persistence`) say which.
- **FR-008**: Failures have stable codes and exits, reusing order info's set (`configuration_*`, `protocol_unsupported`, `pow_required`, `request_rejected`, `reply_timeout`, `node_refused` with `reason`, `reply_malformed`, `relay_*`) plus the new codes the plan needs (declined confirmation, save failure). A timeout after publication is `uncertain`, not `error`, for this command.
- **FR-009**: One versioned JSON outcome with canonical command `order.new`; success data lists the created order with explicit documented fields; no trade private key, mnemonic or database record is serialized. Human and JSON describe the same order.
- **FR-010**: Command-scoped schemas and fixtures, regenerated discovery, drift, boundary and CI checks as for order info; the pilot and info documents do not change.
- **FR-011**: All publication and reception go through `msg`; no new direct send, fetch or subscribe appears elsewhere. Text from the node is untrusted and escaped. Diagnostics follow the allow-list and never include keys, relays, arguments or invoices.
- **FR-012**: The legacy `neworder`, `cli/new_order.rs` and its prompt are removed with no alias after the new command is verified; the unmigrated write commands keep rejecting JSON early.

## Success Criteria

- **SC-001**: Against the scripted daemon, an accepted order yields exactly one valid document, one published request, a saved local order row and a consumed trade index; every refusal, timeout and argument failure yields one document with its documented code and exit.
- **SC-002**: With no `--yes`, JSON mode does nothing (no index consumed, no database write, no publication); closed or redirected stdin is never read.
- **SC-003**: A forced save failure after an accepted reply produces `uncertain` with the order id and no retry; a reply that never arrives after publication produces `uncertain` with `daemon_acceptance` unknown.
- **SC-004**: Local failures before the reservation (arguments, configuration, identity, declined confirmation, unwritable store) leave the database byte-identical, including the trade index. Failures after it that send nothing (work, protocol, rejected or unreachable relay) leave the orders and the raw trade index unchanged (the reservation is given back; the file itself is written), or skip the index when another command reserved one meanwhile (amended 2026-11-05 by spec 008).
- **SC-005**: Human and JSON agree on every returned field; no secret sentinel appears in any stream; Linux and the CI-hosted Windows runner pass.
- **SC-006**: Nothing about a real daemon is claimed until a separately authorized regtest probe is recorded.

## Assumptions And Open Items For The Plan

- The reply shapes (`new-order` with the order, `CantDo` reasons) come from the daemon source and `mostro-core`; a scripted fake daemon encodes them, as for order info.
- The trade-index rule (reserve right before publish, keep it consumed even when the outcome is `uncertain`) and whether a provably rejected request returns the index need a decision in the plan after reading the daemon's behavior.
- Range orders: the daemon may later publish a child order without a request id; that follow-up is out of scope here.
- Windows follows CI; regtest probe needs separate authorization.
