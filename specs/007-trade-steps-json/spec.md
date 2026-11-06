# Feature Specification: Trade Steps For Humans And Agents

**Feature**: `007-trade-steps-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-02

**Status**: Implemented and verified on Linux against a scripted node (slices 1-6, 2026-11-02); CI is checked after the push. Probed on the maintainer's local regtest node on 2026-11-02 (a whole trade, the refusals, plain and cooperative cancel; see `tests/TEST_SUMMARY.md`); a range order and `uncertain` were not probed. Range continuation is the required spec 008. Progress is tracked only in [tasks.md](tasks.md)

**Input**: Seventh step of the AI-ready CLI roadmap: canonical `order fiat-sent`, `order release` and `order cancel`
(replacing `fiatsent`, `release`, `cancel`), with human and JSON output and the conventions of
[add invoice](../005-invoice-json/spec.md) (stored trade key, `msg::exchange`, correlation by order id, `uncertain`, `cli/write.rs`).
With [order take](../004-order-take-json/spec.md) and [message list](../006-message-list-json/spec.md) a CLI user can then finish a
trade without the legacy commands. None of those documents change.

## Clarifications

### Session 2026-11-02 (defaults, open to veto)

- Q: What are the three commands? -> A: `order fiat-sent --order UUID` (the buyer says the fiat was sent; node: order must be `active`,
  sender must be the buyer), `order release --order UUID` (the seller settles the hold invoice; node: status `fiat-sent` or `dispute`,
  sender must be the seller), `order cancel --order UUID` (maker or taker; the node decides between a plain cancel and the first or second
  step of a cooperative cancel from the order status). The CLI does not decide which: it reports what the node did.
- Q: Confirmation? -> A: The rule of `order new` for all three: JSON mode requires `--yes`, never prompts or reads stdin, and without it
  fails with `invalid_arguments` doing nothing; human mode previews and asks. `release` is irreversible and `cancel` can lock funds
  back; `fiat-sent` is cheap to undo only by dispute, so it is not exempt.
- Q: Trade index and range orders? -> A: Decided 2026-11-02: these commands sign with the order's stored trade key and spend no index, and they
  do **not** send a next trade key. On a range order of the user's with room left, `fiat-sent` and `release` therefore leave the range
  **not continued** by the node (no child order is created). (Superseded by spec 008, which sends the key and removes the warning `range_not_continued`.) Doing this correctly (reserving the index, sending the next trade key, and
  giving the child order a usable local row) is **required follow-up work, spec 008 "range children"**, not an option; this spec stays free
  of it so the three commands can be finished and verified first. The room test is the shared `range_has_room` rule (already in the store). Amended by [spec 008](../008-range-children/spec.md): the rule now also requires the order to be the user's own, which the first version could not tell (a taker of a range order got the warning too).
  The old command waited for the child's `new-order` message; neither this spec nor 008 needs the command to wait, `message list` shows it.
- Q: Local effects? -> A: After an accepted reply the local row status follows the node (`fiat-sent`; `settled-hold-invoice`; `canceled`,
  `cooperatively-canceled`; a cooperative cancel started by either side leaves the status as it is). A failed update is `ok` with the
  warning `local_status_stale`, as in `order add-invoice`: the node already acted and a retry is refused.
- Q: What does silence mean? -> A: `uncertain` with the order id, never retried, for all three. The node may have settled or cancelled.
- Q: Paying, rating, disputes? -> A: Out of scope. `rate` and `dispute` are later specs; the `rate` message the node sends after a
  release is only visible through `message list`.

## Scope

Each command publishes one signed request with the stored trade key of the order, waits for the correlated reply and reports what the node
did. Out of scope: creating or paying invoices, `rate`, `dispute`, admin commands, continuing a range order (spec 008, required), MCP.

## Requirements

- **FR-001**: `order fiat-sent --order UUID [--yes]`, `order release --order UUID [--yes]`, `order cancel --order UUID [--yes]`. The id is a UUID,
  canonicalised to lowercase. `fiatsent`, `release`, `cancel`, `-o` and `--order-id` are removed with no alias. Invalid values are
  `invalid_arguments` before any effect.
- **FR-002**: Identity from the existing store, read-only; none means `identity_missing`. The trade key comes from the stored row; an unknown
  order or a row without a key is `order_unknown` and nothing is published.
- **FR-003**: Confirmation as clarified above; the preview shows the command, the order id and the trade public key.
- **FR-004**: Exactly one signed request is published, no retry. The trade index is never changed, except by the continuation of a range order (spec 008) (amended 2026-11-05 by spec 008).
- **FR-005**: The reply is selected by node author, addressee (the trade key), allowed actions and the request id (the node echoes it to the
  sender in all three source paths, so there is no order-id fallback); the order the reply names is checked afterwards. Armed before publishing, bounded by a deadline, released on every
  exit, through `msg::exchange`. Allowed: `fiat-sent` -> `fiat-sent-ok`, `cant-do`; `release` -> `hold-invoice-payment-settled`, `released`
  (as the node sends it), `cant-do`; `cancel` -> `canceled`, `cooperative-cancel-initiated-by-you`, `cooperative-cancel-accepted`, `cant-do`.
- **FR-006**: Outcomes: `ok` when the node accepted; `error` when it provably refused (`node_refused` + reason, e.g. `invalid_pubkey`,
  `not_allowed_by_status`, `order_already_canceled`, `is_not_your_order`) or nothing was published; `uncertain` when published and the result
  is unknown (no reply, malformed reply, relay read failure). A `cant-do` is a refusal only when it names the order (mostrod echoes it); one
  that names another order or none is a malformed reply, `uncertain` (amended 2026-11-05 after review, as for `dispute open` and `order rate`).
- **FR-007**: Data per command (see the plan): the order id, a `result` word (`fiat_sent`; `released`; `canceled`, `cancel_requested`,
  `cancel_accepted`), the trade public key, and, for a range order, the reserved `next_trade_index` (spec 008). A `next_step` names only commands that exist (for
  cancel_requested: wait for the peer, who answers with `order cancel`; for fiat-sent: wait for the seller to release).
- **FR-008**: One versioned JSON outcome per command (`order.fiat-sent`, `order.release`, `order.cancel`, names already in the registry),
  command-scoped schemas, fixtures, regenerated discovery, drift and boundary checks. No private key, mnemonic or database record is
  serialized. Human and JSON describe the same data.
- **FR-009**: All publication and reception go through `msg`. Node text is untrusted and escaped; diagnostics never include keys, relays or
  arguments.
- **FR-010**: The legacy `fiatsent`, `release`, `cancel` arms of `cli/send_msg.rs` and their `print_dm_events` path are removed with no alias
  after the new commands are verified. `dispute`, `admcancel`, `admsettle`, `admaddsolver` keep using `send_msg.rs` until their own specs, so the
  file is trimmed, not deleted. Unmigrated write commands keep rejecting JSON early.

## Success Criteria

- **SC-001**: Against the scripted daemon, each accepted reply (including both cooperative-cancel steps) yields one valid document and one
  published request, with the local status updated; every refusal, silence and argument failure yields one document with its code and exit.
- **SC-002**: Without `--yes`, JSON mode does nothing and never reads stdin.
- **SC-003**: Every failure leaves the database byte-identical; the trade index never changes (a range continuation, spec 008, reserves and may give back an index) (amended 2026-11-05 by spec 008).
- **SC-004**: A reply for another addressee, another author or with another request id is ignored; a reply naming another order is `reply_malformed`.
- **SC-005**: Human and JSON agree on every field; no secret sentinel appears in a diagnostic stream; Linux and the CI Windows runner pass.
- **SC-006**: Nothing about a real daemon is claimed until a separately authorized regtest probe is recorded. The probe can walk a whole trade
  (take, pay, fiat-sent, release) on the maintainer's regtest node with a paying seller.

## Assumptions And Open Items For The Plan

- Reply shapes come from node v0.20.1 (`fiat_sent.rs`, `release.rs`, `cancel.rs`); the scripted daemon encodes them. Whether `released` or
  `hold-invoice-payment-settled` reaches the seller on regtest v0.19.3 needs the probe.
- Release on a `dispute` order is accepted by the node; the CLI does not forbid it.
- Cashu escrow replies are not modeled (`reply_malformed`, uncertain), as in `order take`.
- The status mapping for the local row (settled-hold-invoice vs success) is checked against `mostro-core::Status` in the plan.
