# Feature Specification: Add Invoice For Humans And Agents

**Feature**: `005-invoice-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-01

**Status**: Implemented and verified on Linux against a scripted fake daemon (slices 1-6, 2026-11-01); CI (Linux and the native Windows runner) was green through slice 4b when this was written; `order add-invoice` was probed on the maintainer's local regtest node on 2026-11-01 (see `tests/TEST_SUMMARY.md`); `bond add-invoice` was not. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Fifth step of the AI-ready CLI roadmap: canonical `order add-invoice` (replacing `addinvoice`) and `bond add-invoice`
(replacing `addbondinvoice`), with human and JSON output and the `order take-sell` conventions. Builds on
[order take](../004-order-take-json/spec.md) (confirmation, `uncertain`, `cli/write.rs`, `store`, `msg::exchange`),
[order new](../003-order-new-json/spec.md), [order info](../002-order-info-json/spec.md) and the
[order-list pilot](../001-order-list-json/spec.md); none of their documents change.

## Clarifications

### Session 2026-11-01

- Q: What are the two commands? -> A: `order add-invoice` answers the node's request for the buyer's payout invoice on an order the user
  already took or made (`add-invoice` is what `order take-sell` and a maker's sell-order take ask for). `bond add-invoice` answers the node's
  request for the user's share of a **slashed bond** (`add-bond-invoice`); it has nothing to do with `pay-bond-invoice`, the taker or maker
  bond the user must pay. `order new` and `order take-*` already return that bond invoice as data (amendment 2026-11-01); paying it is done with any Lightning wallet
  and is out of scope here. The two are specified together because they share the lookup of the stored trade key, one
  signed request and a correlated reply (decided by the maintainer, 2026-11-01).
- Q: What does silence from the node mean for `bond add-invoice`? -> A: `uncertain` (decided 2026-11-01). The legacy command read a timeout as
  success because older nodes send no acknowledgement. Node v0.20.1 sends `bond-invoice-accepted`; without it the program cannot know the
  invoice was accepted, so it reports `uncertain` with the order id and never claims success.
- Q: Confirmation and the trade index? -> A: Defaults, open to veto: the rule of `order new` and `order take` (JSON requires `--yes` and never
  reads stdin; human mode previews and asks), because the invoice fixes where the user's sats go. Neither command uses a new trade index: the
  request is signed by the order's stored trade key and carries no index, so the index is never advanced and a failed run leaves the database
  byte-identical.
- Q: What if the local save of an accepted `order add-invoice` fails? -> A: Default, open to veto: the status is `ok` with a warning
  `local_status_stale`, not `uncertain`. The node accepted; the local row is only a cache and a retry is refused by the node (`not_allowed_by_status`),
  so reporting `uncertain` would invite a pointless retry.
- Q: Is paying anything part of these commands? -> A: No. The commands submit an invoice; they never create or pay one.

## Scope

`order add-invoice --order UUID --invoice INVOICE` and `bond add-invoice --order UUID --invoice BOLT11` publish one signed reply with the
stored trade key of that order, wait for the correlated answer and report what the node did. Out of scope: creating an invoice, paying a hold
invoice or a bond, `pay-bond-invoice`, any later step of the trade, a refresh of the local row from the book, MCP.

## Requirements

- **FR-001**: `order add-invoice --order UUID --invoice INVOICE [--yes]` (BOLT-11 or Lightning address, as `take-sell --invoice`) and
  `bond add-invoice --order UUID --invoice BOLT11 [--yes]` (BOLT-11 only; a Lightning address is `invalid_arguments`, the node needs the exact
  amount). The order id is a UUID, canonicalised to lowercase. `addinvoice`, `addbondinvoice`, `-o`, `--order-id`, `-i` are removed with no alias.
  Invalid values are `invalid_arguments` before any effect.
- **FR-002**: Identity from the existing store, read-only; none means `identity_missing`. The trade key comes from the stored row of that order;
  an unknown order, or a row without a key, is `order_unknown` (new code, exit 1) and nothing is published. The secret never leaves `store`.
- **FR-003**: Confirmation as clarified above; the preview shows the command, the order id and the trade public key, never the invoice.
- **FR-004**: Exactly one signed request is published, with the order id and no trade index. No retry, no second publication.
- **FR-005**: The reply is selected by the author (the node), the addressee (the order's trade key), the allowed actions and the **order id**,
  armed before publishing, bounded by a deadline, released on every exit, through `msg::exchange`. The request id is matched when the node
  echoes it. Source check: `bond-invoice-accepted` is sent with no request id, so the order id must be an accepted correlation for this
  command. Allowed: `order add-invoice` -> `waiting-seller-to-pay`, `hold-invoice-payment-accepted`, `invoice-updated`, `cant-do`;
  `bond add-invoice` -> `bond-invoice-accepted`, `cant-do`.
- **FR-006**: Outcomes (see the [data model](data-model.md)): `ok` when the node accepted; `error` when it provably refused (`node_refused` +
  its reason, e.g. `invalid_invoice`, `invalid_peer`, `not_allowed_by_status`) or nothing was published; `uncertain` when published and the
  result is unknown (no reply, malformed reply, relay read failure), including silence for `bond add-invoice`.
- **FR-007**: On an accepted `order add-invoice` the local row status is updated from the reply (`waiting-seller-to-pay` -> `waiting-payment`;
  `hold-invoice-payment-accepted` -> the status of the order it carries; `invoice-updated` -> unchanged). `bond add-invoice` has no local effect.
  A failed update is the `local_status_stale` warning.
- **FR-008**: One versioned JSON outcome per command (`order.add-invoice`, `bond.add-invoice`, names already in the registry), command-scoped
  schemas, fixtures, regenerated discovery, drift, boundary and CI checks. No private key, mnemonic, database record or the invoice is serialized.
  Human and JSON describe the same data.
- **FR-009**: All publication and reception go through `msg`. Node text is untrusted and escaped. Diagnostics follow the allow-list and never
  include keys, relays, arguments or invoices.
- **FR-010**: The legacy `addinvoice`, `addbondinvoice`, `cli/add_invoice.rs`, `cli/add_bond_invoice.rs` and their `print_dm_events` path are removed
  with no alias after the new commands are verified; the unmigrated write commands keep rejecting JSON early.

## Success Criteria

- **SC-001**: Against the scripted daemon each accepted reply yields exactly one valid document and one published request, with the local status
  updated for `order add-invoice`; every refusal, silence and argument failure yields one document with its documented code and exit.
- **SC-002**: Without `--yes`, JSON mode does nothing and never reads stdin.
- **SC-003**: Every failure leaves the database byte-identical; the trade index is never changed.
- **SC-004**: A reply without a request id is matched for `bond add-invoice`; a reply for another order or another addressee is ignored.
- **SC-005**: Human and JSON agree on every field; no secret sentinel and no invoice appears in a diagnostic stream; Linux and the CI Windows
  runner pass.
- **SC-006**: Nothing about a real daemon is claimed until a separately authorized regtest probe is recorded. Regtest (v0.19.3) can probe
  `order add-invoice`; `bond add-invoice` cannot be probed there (no bonds).

## Assumptions And Open Items For The Plan

- Reply shapes come from node v0.20.1 (`add_invoice.rs`, `bond/payout.rs`, `util.rs`) and `mostro-core`; the scripted daemon encodes them. The
  `bond-invoice-accepted` payload is an order with the counterparty share as amount.
- Whether `waiting-seller-to-pay` answers `order add-invoice` on regtest, and whether the request id is echoed, need the probe.
- Wrong-peer, wrong-status and wrong-amount are node rules seen only as refusals.
- The minimal row left by `take-sell --invoice` has no invoice stored; adding one later is the same command and needs no local invoice column.
