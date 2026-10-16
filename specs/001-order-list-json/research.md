# Research: Order-List JSON Pilot

**Date**: 2026-10-16 | **Status**: Source-grounded design, not runtime verification.

SDK findings refer to locally resolved nostr-sdk 0.45.4 / nostr 0.45.5 sources;
recheck on lockfile changes. No production network or real identity was used.

## 1. Selection Semantics

**Evidence**: [wrapper](../../src/cli/list_orders.rs),
[parser](../../src/parser/orders.rs), [tag decoder](../../src/nip33.rs),
[event helpers](../../src/util/events.rs), [tests](../../tests/parser_orders.rs).

**Decision**: Preserve kind 38383, author, `#z=order`, seven-day window, limit 50
per relay and no `until`. Status defaults to pending; no all-statuses option.
Side/status are case-insensitive without trimming; currency is uppercased without
trimming/validation. Empty currency remains an exact filter. Reduce by UUID
before filtering. Newer timestamps win; same-UUID/time ties retain the lowest
event ID through SDK set ordering. Cross-UUID timestamp ties remain unspecified.

Preserve permissive tags: bad/missing ID or side skips events; absent status is
None, invalid/valueless status is pending; invalid amounts/premium become zero;
incomplete range endpoints stay nullable. No new positivity/range validation or
malformed-tag warning counter. Enforce the requested filter at the transport.

**Rationale**: Latest-before-filtering avoids stale pending orders. Preserve
business behavior while changing output and transport failure semantics.

**Alternatives considered**: Relay-side status filtering changes reduction;
strict tags or new tie ordering belong in separate behavioral work.

## 2. Empty Results And Completion

**Evidence**: `fetch_events_list` uses a 15-second SDK fetch. Resolved
`FetchEvents::into_future` logs per-relay errors and returns collected events.
Auto-close timeout/disconnect can end streams without error items;
`connect().and_wait(...)` returns unit, not proof of connectivity.

**Decision**: Arm notifications before explicit subscriptions; require every
configured relay's EOSE and freeze each relay at its EOSE. Any incomplete/failed
relay means error/data null even when others yielded events. Dedup relay URLs
and event IDs, retain SDK verification/filter enforcement, cap unique events at
10,000, and error on overflow. Allow 10 seconds connect, 15 seconds read and
1 second cleanup. Always unsubscribe/disconnect; no retries.

**Rationale**: All-relay completion conservatively satisfies FR-005, correcting
old best-effort error handling. EOSE still does not prove exhaustive history.

**Alternatives considered**: Outer timeouts and switching only to stream_events
cannot expose swallowed failures. Partial success with warnings is not chosen.
Prove EOSE-empty, CLOSED and silent-relay cases first; if notifications cannot
expose them, revise the adapter instead of weakening the contract.

## 3. Public Reads Without Identity

**Evidence**: [startup](../../src/cli.rs) initializes full Context;
[connector](../../src/util/net.rs) already uses ephemeral NIP-42 keys.

**Decision**: Resolve only public key/relays, CLI over environment, no `set_var`.
No SQLite, trade keys, admin credentials, directories or DM protocol probe.
Schema bypasses configuration. Reuse extracted TLS/relay helpers; keep legacy
Context until its own migration.

**Rationale**: Public events need no trading identity or DM negotiation.
Ephemeral relay authentication is not trade publication.

**Alternatives considered**: A disposable DB still has effects; a duplicated
connector violates reuse. Do not migrate unrelated DB/DM code here.

## 4. Output Intent On Parse Failure

**Evidence**: [CLI](../../src/cli.rs) has message strings, hyphen values and
colliding shorts. Clap ignore_errors can stop before a later output flag.

**Decision**: Full Clap parsing remains authoritative. Use a metadata-led
bootstrap over OsString only for output intent/resolvable command identity.
Respect scopes, arities, attached/variadic/hyphen values, shorts and terminators.
Never remove tokens or infer commands from value text. Missing/invalid/conflicting
output declarations or ambiguous recovery use safe stderr/exit 2; otherwise an
explicit JSON request receives an invocation envelope. Reject duplicate output
declarations rather than invent last-value precedence.

**Rationale**: Naive scans confuse message text with flags; raw Clap diagnostics
can echo secrets. Prove this boundary with adversarial grammar tests first.

**Alternatives considered**: Infallible exits, substring scans, stripped argv and
partial parsing alone cannot meet the contract.

## 5. Public Fields And Human Parity

**Evidence**: SmallOrder uses optional identity/status and signed i64 amounts;
the table includes premium and event time. Missing-status display is active,
but listing always supplies a status filter, excluding absent-status rows.

**Decision**: Require ID/side/status on selected DTOs; preserve general reducer
optionality for other callers. Map explicit public fields, premium, fixed/range
amounts and time. Monetary i64 values are decimal strings; zero sats means
market-priced. Pair nullable UTC RFC3339 created_at with lossless signed
created_at_unix. Invalid calendar conversion retains the integer, adds one safe
warning and keeps the human Invalid date display.

**Rationale**: Filter invariants resolve missing status without inventing active
status in JSON or exposing future secret-bearing core fields.

**Alternatives considered**: Whole-core/DB serialization couples the contract to
internal changes; floats lose precision; stricter tags exceed scope.

## 6. Generated Offline Discovery

**Evidence**: [manifest](../../Cargo.toml) includes Clap/Serde; derive currently
arrives through feature unification. No schemars/schema validator is in the CLI lock.

**Decision**: Explicit Serde derive; propose schemars 1.2.3, defaults off,
derive/std only. Its minimum is 1.74; verify the resolved graph against 1.86.
Primitive DTOs avoid UUID/chrono schema features. Generate Draft 2020-12 schemas
and Clap/shared-validator input metadata. schema_version is string `1`, separate
from package version. No extra runtime validator crate just for generation tests.

Human schema presentation is pretty-printed raw discovery JSON as the roadmap
prescribes; explicit JSON wraps it in an outcome. Default mode remains human.
Schema documents are JSON objects, avoiding recursive schema generation.

**Rationale**: Serde cannot generate schemas; one derive library avoids a custom
framework. Drift checks and semantic tests complement generation.

**Alternatives considered**: Permanent handwritten schemas drift; a generic
schema generator adds maintenance. Planning examples are proposed, not coverage.

## 7. Metadata And Compatibility

**Decision**: One capability registry owns canonical names, Clap paths, JSON
support/result schemas. Legacy commands report roadmap canonical names and
output_not_supported; unresolved paths use command null. Read-only metadata is
not_applicable for publication/acceptance/persistence. Legacy rejection uses
not_attempted/unknown/not_attempted. Diagnostics are allow-listed, never raw
chains/URLs/argv/server text, including verbose. Serialize before stdout; write
failure is nonzero and never appends another document. Help/version and invalid
output selection are text exceptions; crash/kill/partial writes cannot promise JSON.

**Rationale**: Shared coverage drives dispatch/discovery. Envelope v1 reserves
uncertain for future writes; pilot command schemas allow only ok/error.

**Alternatives considered**: Guessed identity, data-only MCP results and retry on
ambiguity are rejected. Keep old MCP paired with its older CLI until migration;
do not invent release versions here.

## 8. Verification

**Decision**: Reuse argument/parser/filter tests; one Rust process-test home with
isolated homes/environment, closed stdin, captured streams and parent watchdogs.
Use SDK loopback/transport fixtures, dev-only features and synthetic keys. Cover
EOSE-empty, CLOSED, silent timeout and partial-relay failure on both platforms.

**Rationale**: This avoids MCP's Unix fixture blocker. Earlier CLI test results
predate this pilot and later user edits, so cannot certify the proposed work.

**Alternatives considered**: Shell executables, guessed unused ports and public
relays are nondeterministic or platform-specific. MCP migration is out of scope.
Design choices are resolved; baseline/runtime proof obligations remain gates.
