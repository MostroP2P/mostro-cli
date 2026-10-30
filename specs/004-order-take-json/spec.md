# Feature Specification: Order Take For Humans And Agents

**Feature**: `004-order-take-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-10-30

**Status**: Clarified; not planned. Next: research (reply shapes from the daemon source), plan, data model, CLI contract, then lean slices in `tasks.md`.

**Input**: Fourth step of the AI-ready CLI roadmap (critical-path step 4): canonical `order take-sell` and `order take-buy`,
replacing `takesell` and `takebuy`, with human and JSON output, the confirmation rule of `order new`, the `uncertain` status and a
local save. Builds on [order new](../003-order-new-json/spec.md) (trade key and index handling, `store`, `uncertain`),
[order info](../002-order-info-json/spec.md) and the [order-list pilot](../001-order-list-json/spec.md); none of their documents change.

## Clarifications

### Session 2026-10-30

- Q: How are the commands spelled, given the approved canonical names `order.take-sell` and `order.take-buy` (contract v1)? -> A: Two subcommands, `order take-sell` and `order take-buy` (decided by the maintainer, 2026-10-30). Each path carries its own canonical name, so no registry or contract change is needed and an argument error can always name its command. The roadmap's single `order take --side` form is not used. As elsewhere, the side names the order taken: `take-sell` takes a sell order (the taker buys sats), `take-buy` takes a buy order (the taker sells sats).
- Q: What confirmation rule applies? -> A: The rule of `order new` (decided 2026-10-30): JSON mode requires `--yes`, never prompts and never reads stdin, and without it fails with `invalid_arguments` (exit 2) doing nothing; human mode shows a preview and asks (blank or `y` accepts, anything else and closed stdin decline, `order_declined`). This adds a prompt that the legacy commands did not have.
- Q: How is the local row saved when a take-sell carrying the buyer's invoice is answered by `waiting-seller-to-pay` with no order (#204)? -> A: A minimal row built from what the command knows (decided 2026-10-30): the order id, the side, the status `waiting-payment`, the fiat amount if one was given and the trade key, saved with the trade index in one transaction. There is no second network read; the command stays one request on one connection. Fields the node did not send stay empty until a later refresh.
- Q: What about `order new`'s open conventions? -> A: They carry over unchanged: no identity means `identity_missing` and nothing is created; the trade index is advanced whenever the request was published and the node did not provably refuse it; a published request with an unknown result, or an accepted take whose local save failed, is `uncertain` and is never retried by the program.
- Q: Is paying anything part of this command? -> A: No. A hold invoice that the node returns to a seller is returned as data for the caller to pay; the buyer's invoice for a take-sell is only submitted, never created. Taker bonds are not paid (`bond_required`, as for `order new`). The follow-up commands (`addinvoice`, `fiatsent`, `release`, ...) are later specs.

## Scope

`order take-sell` and `order take-buy` publish one signed take request with a fresh trade key, wait for the correlated reply, save the
trade locally and return what the node asked the taker to do next. Out of scope: paying the hold invoice or a bond, adding an invoice
afterwards, any later step of the trade, range-order child orders, MCP.

## Requirements

- **FR-001**: `order take-sell --order UUID [--invoice INVOICE] [--amount FIAT] [--yes]` and `order take-buy --order UUID [--amount FIAT] [--yes]`. The order id is a UUID, canonicalised to lowercase; `--amount` is the fiat amount for a range order and is validated as a positive integer; `--invoice` is a BOLT-11 invoice or a Lightning address, bounded in size, with no control characters or whitespace. The plan fixes the exact grammar. `takesell`, `takebuy`, `-o`, `--order-id` and the short flags are removed with no alias. Invalid values are `invalid_arguments` before any effect.
- **FR-002**: Identity comes from the existing store, read-only first; no identity means `identity_missing` and nothing is created.
- **FR-003**: Confirmation as clarified above.
- **FR-004**: Exactly one signed request is published, with a trade key derived at the next index, reserved only right before publishing. No retry, no second publication, no extra network read.
- **FR-005**: The reply is selected by correlation (request id, node author, addressed to the trade key, allowed actions), armed before publishing, bounded by a deadline, released on every exit, reusing `msg::exchange`. The allowed actions are those the daemon answers a take with: `add-invoice`, `waiting-seller-to-pay`, `pay-invoice`, `pay-bond-invoice` and `cant-do`.
- **FR-006**: The outcome says what the node asked next and carries it as data: for a take-sell the order awaiting the buyer's invoice (`add-invoice`) or the confirmation that the invoice was accepted (`waiting-seller-to-pay`); for a take-buy the hold invoice the seller must pay (`pay-invoice`). The hold invoice is returned as given by the node and is never logged.
- **FR-007**: On an accepted take the order and the trade index are saved in one transaction (the minimal row where the reply carries no order). If the save fails the outcome is `uncertain` with the order id and public trade data; the trade index is still spent when possible.
- **FR-008**: Statuses and stages follow `order new`: `ok` only when the node accepted and the save succeeded; `error` when the take was provably not accepted or never published (including `node_refused` with the node's reason, `bond_required`); `uncertain` when published with an unknown result or accepted but not saved. A refusal spends no index.
- **FR-009**: Failures have stable codes and exits, reusing the `order new` set; this command adds none beyond what the plan shows necessary. Cashu escrow requests, if the node sends them, are not supported and are reported as `reply_malformed` (uncertain) until a later spec.
- **FR-010**: One versioned JSON outcome per command with canonical names `order.take-sell` and `order.take-buy`; data lists the order, the next step, the trade index and the trade public key; no private key, mnemonic or database record is serialized. Human and JSON describe the same data.
- **FR-011**: Command-scoped schemas and fixtures per command, regenerated discovery, drift, boundary and CI checks as for the earlier commands; existing documents do not change.
- **FR-012**: All publication and reception go through `msg`; no new direct send, fetch or subscribe appears elsewhere. Text from the node is untrusted and escaped. Diagnostics follow the allow-list and never include keys, relays, arguments, invoices or hold invoices.
- **FR-013**: The legacy `takesell`, `takebuy`, `cli/take_order.rs` and their special save path are removed with no alias after the new commands are verified; the unmigrated write commands keep rejecting JSON early.

## Success Criteria

- **SC-001**: Against the scripted daemon, each accepted take (take-sell without and with an invoice, take-buy, a range take with `--amount`) yields exactly one valid document, one published request, a saved local row and a consumed trade index; every refusal, timeout and argument failure yields one document with its documented code and exit.
- **SC-002**: With no `--yes`, JSON mode does nothing (no index consumed, no database write, no publication); closed or redirected stdin is never read.
- **SC-003**: A forced save failure after an accepted reply and a missing reply each produce `uncertain` with no retry; the index is spent in both.
- **SC-004**: Local failures and refusals leave the database byte-identical, including the trade index.
- **SC-005**: Human and JSON agree on every returned field; no secret sentinel and no hold invoice appears in a diagnostic stream; Linux and the CI-hosted Windows runner pass.
- **SC-006**: Nothing about a real daemon is claimed until a separately authorized regtest probe is recorded.

## Assumptions And Open Items For The Plan

- Reply shapes (`add-invoice` with the order, `waiting-seller-to-pay`, `pay-invoice` with the order and hold invoice, `pay-bond-invoice`, `cant-do`) come from the daemon source (v0.20.1) and `mostro-core`; a scripted fake daemon encodes them. The `waiting-seller-to-pay` reply carries no order and its message id is the order id.
- The daemon's own rules appear to the CLI only as refusals: a taker with a pending order, a non-pending or expired order, taking one's own order, an amount outside the range, a stale price. The plan lists the reasons the fake daemon models.
- Whether `waiting-seller-to-pay` can also answer a take-sell without an invoice (it is documented as the reply when the invoice is present) and what a node with Cashu escrow enabled sends need the regtest probe.
- The minimal row's missing fields (currency, payment method, amounts) and how a later refresh fills them are noted for the plan; no refresh is part of this feature.
- Windows follows CI; the regtest probe needs separate authorization.
