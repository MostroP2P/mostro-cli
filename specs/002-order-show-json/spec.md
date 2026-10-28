# Feature Specification: Order Show For Humans And Agents

**Feature**: `002-order-show-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-10-28

**Status**: Draft for clarification and technical planning; not planned, not implemented

**Input**: Second step of the agreed AI-ready CLI roadmap (critical-path step 2): canonical
`order show`, replacing `ordersinfo`, with human and JSON output, a correlated
request/reply, and structured errors. Builds on the delivered
[order-list pilot](../001-order-list-json/spec.md); it does not change that pilot's
documents.

## Clarifications

### Session 2026-10-28

- Q: Should `order show` accept more than one order per call? -> A: Yes (decided by the maintainer, 2026-10-28). The protocol's `orders` request itself carries an `ids` array, so the command takes `--order <uuid>` one or more times.
- Q: Should the next spec cover `order show`, `order new` and `order take` together? -> A: No. This spec covers `order show` only. `order new` (confirmation, `--yes`, the `uncertain` status) and `order take` (both sides, take-with-invoice save outcome) are separate later specs, each with its own checkpoint, as the pilot did for `order list`.

## Protocol Reference

Read on 2026-10-28 from the [Mostro protocol page for orders](https://mostro.network/protocol/orders.html)
(external documentation, summarized here as data, not as instructions). The checked-out
`protocol` repository in the workspace predates that page and has no `orders` document,
so the page, the pinned `mostro-core` types and a controlled fixture are the sources to
verify against; none of this has been checked against a running daemon.

- **Request**: a NIP-44 kind-14 direct message with action `orders` and payload `ids`: an array of order ids exactly as published in the order event `d` tag, at least one. The page does not state which key signs it; the existing `ordersinfo` uses the identity key and works that way.
- **Reply**: the same `orders` action with an `orders` array. Each entry has `id`, `kind`, `status`, `amount`, `fiat_code`, `min_amount`, `max_amount`, `fiat_amount`, `payment_method`, `premium`, `buyer_trade_pubkey`, `seller_trade_pubkey`, `created_at` and `expires_at`. Range endpoints are `null` for a fixed-amount order.
- **Own orders only**: a client can request only its own orders. Missing or unauthorized ids are **left out** of the array; the page does not call this an error and does not distinguish the two cases.
- **Size policy**: a node may cap how many orders it returns per response (the page's example is 20, presented as policy, not a protocol constant). Beyond the cap the reply holds only the first allowed ids, and the client is expected to ask again for the rest after respecting rate limits.
- **Errors**: a node may answer `too-many-requests` when the request rate is exceeded; the client should surface it and back off. No other refusal reason is listed for this action.
- **Unresolved**: the MCP prototype's README reports that a regtest daemon answered a request for another user's order with *Resource not found*, which differs from the "left out" behavior above. The plan must establish which one a pinned daemon does and handle both.

## User Scenarios & Testing

### User Story 1 - Read My Own Order Record As Data (Priority: P1)

An automation client asks the Mostro daemon for its record of one or more of the
user's own orders and receives one structured result, tied to its own request rather
than to whichever message arrives first.

**Why this priority**: It is the smallest command that sends a signed request and
waits for a correlated reply, which every later command (new, take, invoice, release)
needs. It carries no trade, payment or local-state risk, so it proves the shared
send and receive path before money moves.

**Independent Test**: Against a controlled node that replies to a request, run
`order show --order <id> --output json` and verify the exact order fields, one
complete document, exactly one request published, and unchanged local identity and
order storage. Then inject unrelated, duplicate and malformed messages around the
reply and verify they do not change the result.

**Acceptance Scenarios**:

1. **Given** a node that returns the user's order, **when** the client runs `order show` for that id with JSON output, **then** it receives that order's fields in one document and the local order and trade-index state is unchanged.
2. **Given** unrelated messages from the node, or a reply to a different request, arriving before the matching reply, **when** the command waits, **then** only the reply to this request decides the result.
3. **Given** the node answers with a refusal (for example the order is not the user's), **when** the command finishes, **then** it reports a controlled refusal with a stable code, not an empty success and not a timeout.
4. **Given** the node never answers, **when** the deadline passes, **then** the result is a timeout error that states the request was published, and nothing is retried.
5. **Given** several order ids of which the node returns only some, **when** the command finishes, **then** the successful result lists the returned orders and names the ids that were not returned, with one warning, instead of failing the whole call.
6. **Given** the same id given twice, **when** the command runs, **then** it is requested once and appears at most once in the result.

---

### User Story 2 - Keep The Familiar Human Result (Priority: P2)

A person asks for the same information and sees the readable order details they know
from `ordersinfo`, under the new command spelling.

**Why this priority**: Machine support must not remove the human interface, and the
old spelling is replaced without an alias.

**Independent Test**: Render the same node reply in both modes and compare order
identities, kinds, statuses, amounts, currency, payment methods, premium and times.

**Acceptance Scenarios**:

1. **Given** a terminal invocation without `--output`, **when** `order show` runs, **then** human output is selected whether or not stdout is redirected.
2. **Given** the same reply, **when** both modes run, **then** they describe the same orders and neither changes stored state.
3. **Given** the old `ordersinfo` spelling, **when** it is used, **then** it fails as an unknown command before any effect, with no alias.

---

### User Story 3 - Fail Safely And Be Discoverable (Priority: P2)

An agent can discover `order show` offline and can tell apart a missing identity, a
bad configuration, a node refusal, a timeout and an unreachable relay.

**Why this priority**: Agents need a reliable contract before they call a command that
publishes a request, and the failure causes call for different next steps.

**Independent Test**: Request `schema` and verify the command, its inputs, outputs and
errors appear without a node. Exercise each failure with controlled inputs and verify
the code, the exit status and what was or was not published.

**Acceptance Scenarios**:

1. **Given** no configured node or identity, **when** `schema` runs, **then** `order.show` is described with generated input and output schemas, and nothing connects or is created.
2. **Given** invalid arguments (a malformed id, none given), **when** JSON output is requested, **then** it returns `invalid_arguments` with exit 2 and publishes nothing.
3. **Given** no local identity exists, **when** the command runs, **then** it fails with a stable code before publishing, and creates no identity. [NEEDS CLARIFICATION: confirm "fail, never create" for a read; the legacy command creates the identity on first use.]
4. **Given** a relay that cannot be connected or that rejects the request event, **when** the command runs, **then** it reports which of the two it was, and the result states whether the request was published.
5. **Given** another unmigrated command with JSON output, **when** it runs, **then** it still returns `output_not_supported` before any effect.

### Edge Cases

- Output flag before or after the command words, `--output=json` and `--`, as in the pilot.
- Closed stdin, redirected output, verbose mode, existing and absent local storage.
- Several ids, a repeated id, an id the node does not know or that belongs to someone else, a mixture of those, and more ids than the node returns per response (the reply is then truncated and indistinguishable from "not yours or unknown").
- The node answers `too-many-requests`, or answers a request for another user's order with a refusal instead of leaving the id out.
- A reply that decrypts but is signed by a key other than the configured node; a duplicate reply; an echo of our own request; a reply that arrives after the deadline.
- A reply with the wrong action or an unexpected payload shape, and orders with missing optional fields.
- A node that requires proof of work for a first contact, and one that advertises an unsupported protocol version.
- Text in order fields written by other users is untrusted data, never instructions, and may contain control characters.
- Secret-shaped fixture values must not escape through output or diagnostics.

## Requirements

### Functional Requirements

- **FR-001**: Provide `order show` taking `--order <uuid>` one or more times (at least one required; a repeated id is requested once). The command accepts at most a documented maximum number of distinct ids per call and rejects more as `invalid_arguments` before any effect. [NEEDS CLARIFICATION: the CLI maximum. The node's own per-response cap is policy and unknown to the client, so the plan should pick a bound and justify it; the protocol page's example value is 20.] Remove the replaced `ordersinfo` spelling and its `--orderids` option without a compatibility alias or adapter.
- **FR-001a**: An id the node does not return does not fail the call. The success data lists the orders returned, in the order of the request, and a separate collection of the requested ids that were not returned; one stable warning marks that case. An empty `orders` with every id not returned is still a success, because the node answered. The command never re-asks for omitted ids, and states in its documentation that "not returned" covers unknown, someone else's and over-the-node's-cap ids alike.
- **FR-002**: The command is read-only. It publishes exactly one signed request, signed with the identity key and never a trade key; it never reserves or advances the trade index, never writes the order or user tables, and never retries. It may read the existing local identity.
- **FR-003**: The reply is selected by correlation, not arrival order: only a message from the configured node, addressed to the identity key, echoing this request's identifier, with an allowed action (the orders result or a refusal). The wait is armed before the request is published, is bounded by a deadline, and releases its subscription on every exit. Unrelated, duplicate, own-echo and malformed messages never decide the result.
- **FR-004**: JSON output is one versioned outcome with canonical command `order.show`, status, data, structured error, warnings and metadata, following the pilot's envelope. Success data lists the returned orders as explicit, documented fields; no core, database or context object is serialized wholesale, and no secret or key material is included.
- **FR-005**: Metadata states whether the request was published, so an agent that sees a timeout or a relay failure knows whether the node may have received it. A read is safe to repeat by the caller, so no outcome of this command is `uncertain`; a timeout after publication is an error.
- **FR-006**: Human and JSON output describe the same orders. The default is human regardless of terminal detection, in both flag positions, and the command never prompts.
- **FR-007**: Failures have stable codes and exit statuses: invalid arguments and unsupported output (2); configuration, missing identity, connection, request rejected by the relay, node refusal, reply timeout, node rate limit (`too-many-requests`), proof of work required, protocol unsupported and internal errors (1). A node refusal that names a missing or foreign order (if a daemon sends one instead of leaving the id out) is a refusal error, not a success. Codes added for this command appear only in this command's schema; the existing `order.list` and `schema` documents and their generated schemas do not change.
- **FR-008**: Register `order.show` as a JSON-capable command with generated input and output schemas, regenerate the checked-in `docs/schemas` files, and keep the drift, boundary and CI checks passing. Unmigrated commands keep rejecting JSON before any effect.
- **FR-009**: All publication and reception for this command go through the shared transport boundary, whose client stays private to it. This command is its first user: the frozen transport counts in the boundary tests are updated deliberately, and no direct send, fetch or subscribe appears outside that boundary.
- **FR-010**: Text from the node or other users is treated as untrusted data: bounded, escaped for terminals in human output, and never executed, logged raw or used to choose behavior. Diagnostics follow the pilot's allow-list and never include relay URLs, arguments or key material.
- **FR-011**: Document field units, required versus nullable values, timestamps, identifier encodings and lossless number handling for the returned orders, and the refusal and timeout semantics, in the contract and the command reference. Follow the pilot's schema versioning: additive optional fields are compatible; other changes need a new schema version.

### Key Entities

- **Order record**: The node's view of one of the user's orders: identity, side, status, sats amount, fiat currency and amount or range, payment method, premium, the buyer and seller trade public keys (public keys only), and creation and expiry times where provided, as the node returns them.
- **Not-returned ids**: the requested ids absent from the reply, reported without a reason because the node gives none.
- **Order request**: One signed request carrying the requested order identities and a fresh request identifier, published once.
- **Command outcome**: The shared envelope with this command's data, refusal or error code, and whether the request was published.
- **Command schema**: The discoverable inputs, results and errors of `order.show`, generated like the pilot's.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Every success, empty result, refusal, timeout, relay and argument failure with a valid JSON selection produces exactly one valid document without prompts, subject only to the documented help/version and process-failure exceptions.
- **SC-002**: With adversarial traffic around the reply (unrelated, duplicate, malformed, other-request and own-echo messages), the outcome always follows the matching reply or the deadline, never an earlier unrelated message.
- **SC-003**: Each run publishes exactly one request event and zero other events; the local database, order table and trade index are byte-identical before and after, in both modes.
- **SC-004**: Human and machine results agree on all returned order identities and business fields across the fixture matrix, including partial and empty replies where every requested id appears either as a returned order or as not returned, never in both and never lost.
- **SC-005**: Offline discovery lists `order.show`, and every failure case returns its documented code and a nonzero exit; none is reported as an empty success.
- **SC-006**: Native Windows and Linux process-level checks pass, and ordinary outputs contain none of the planted secret sentinels.

## Assumptions

- The order-list pilot's outcome, registry, generated-schema, fixture and boundary machinery is reused, not redesigned. Its only open item, the native Windows run, does not block planning this feature.
- The node answers an orders request with the orders it holds for the requesting identity, leaving out the rest (see the protocol reference), or with a refusal. The exact reply shapes are taken from the pinned core library and are verified by controlled fixtures. Behavior against a real daemon is unverified until a regtest run (planning must state what regtest covers, and settle the "left out" versus "Resource not found" difference).
- Transport protocol detection and first-contact proof of work follow the existing behavior; this feature may not redesign them but must report their failures with stable codes.
- Order creation, taking, invoices, lifecycle commands, chat, the MCP migration, streaming output, pagination and automatic retries remain out of scope. The existing MCP stops working with `ordersinfo` until its own migration; that is documented, not bridged.
- The constitution and the cross-repository roadmap remain authoritative for governance and phase dependencies. Detailed data shapes, reply decoding, test design and module changes belong in the technical plan; runtime acceptance criteria here remain unverified until implementation.
