# Feature Specification: Order New For Humans And Agents

**Feature**: `003-order-new-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-10-29

**Status**: Clarified; not planned. Next: plan, research (reply and `CantDo` shapes from the daemon source, trade-key rules), data model, CLI contract, then lean slices in `tasks.md`.

**Input**: Third step of the AI-ready CLI roadmap (critical-path step 3): canonical `order new`, replacing
`neworder`, with human and JSON output, an explicit confirmation rule, the `uncertain` status and a local save.
Builds on [order info](../002-order-info-json/spec.md) (identity loader, `msg::exchange`, request stages) and the
[order-list pilot](../001-order-list-json/spec.md); neither's documents change.

## Clarifications

### Session 2026-10-29

- Q: With no local identity, should `order new` create one as the legacy `neworder` does? -> A: No (decided by the maintainer, 2026-10-29). It fails with `identity_missing` before any network access and creates nothing, as `order info` does. An agent never creates keys silently.
- Q: How do confirmation and the trade index behave? -> A: In JSON mode `--yes` is required and stdin is never read; without it the command fails with `invalid_arguments` (exit 2) and does nothing. Human mode keeps the preview and the prompt (blank or `y` accepts, `n` cancels, closed stdin is not approval). The trade index is reserved only immediately before the single publish, so every local failure leaves the index and the database untouched (decided 2026-10-29).
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
- **FR-004**: Exactly one signed request is published, with a trade key derived at the next index, reserved only right before publishing and recorded with the order. The identity key proves ownership as in the legacy flow. No retry, no second publication.
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
- **SC-004**: Local failures (arguments, configuration, identity, declined confirmation, work, protocol) leave the database byte-identical, including the trade index.
- **SC-005**: Human and JSON agree on every returned field; no secret sentinel appears in any stream; Linux and the CI-hosted Windows runner pass.
- **SC-006**: Nothing about a real daemon is claimed until a separately authorized regtest probe is recorded.

## Assumptions And Open Items For The Plan

- The reply shapes (`new-order` with the order, `CantDo` reasons) come from the daemon source and `mostro-core`; a scripted fake daemon encodes them, as for order info.
- The trade-index rule (reserve right before publish, keep it consumed even when the outcome is `uncertain`) and whether a provably rejected request returns the index need a decision in the plan after reading the daemon's behavior.
- Range orders: the daemon may later publish a child order without a request id; that follow-up is out of scope here.
- Windows follows CI; regtest probe needs separate authorization.
