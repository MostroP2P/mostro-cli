# Implementation Plan: Order Listing For Humans And Agents

**Branch**: `develop` | **Date**: 2026-10-16 | **Spec**: [spec.md](spec.md)

**Feature directory**: `001-order-list-json`, not a Git branch.

**Status**: Proposed design; no pilot implementation or runtime acceptance evidence.

## Summary

Deliver `order list`, global `--output human|json`, one versioned outcome and
offline `schema`. Preserve order selection and human business fields. Extract
public reads from full identity/database initialization into a private
`msg::Session`. Require per-relay EOSE evidence so failed reads cannot appear as
successful empty books. Reuse decoding, reduction, filtering and table helpers.

Both renderers consume the same allow-listed outcome; `main` owns migrated
stdout. Unmigrated commands remain human-only, rejecting JSON before RPC,
context initialization or other effects. MCP stays a separate, unchanged client.

## Technical Context

**Language/Version**: Rust 2021; declared minimum 1.94 (raised from 1.86 after T009 showed the locked graph needs it); pinned development toolchain 1.97.0. Planning does not verify MSRV or platform compatibility.

**Primary Dependencies**: Existing Clap 4.6.7, mostro-core =0.17.3, resolved nostr-sdk 0.45.4 / nostr 0.45.5, Tokio, futures, Serde/serde_json, chrono, UUID, comfy-table and rustls. Explicitly enable Serde derive. Added in T012: schemars 1.2.3, defaults disabled, only `derive` and `std`, for source-generated DTO schemas; resolved graph checked against MSRV 1.94. Dev-only, already locked: hyper 1.8.1 (`server`, `http1`), hyper-util 0.1.20 (`tokio`) and tungstenite 0.28.0 (`handshake`) since T013, so the harness owns an OS-assigned loopback listener instead of `LocalRelay`'s probe-then-bind port; regex 1.12.3 (`std` only) since T042 to run generated schema patterns in tests, nostr-memory 0.45.2 since T021, to serve a forged event for the signature-verification test.

**Storage**: None for listing/discovery; no DB opening, identity files or migrations. Existing SQLx storage remains on legacy paths.

**Testing**: Existing Rust tests, rstest/serial_test, portable subprocess fixtures and SDK `LocalRelay` fixtures behind the harness-owned loopback listener in `tests/order_list_process.rs`. Dev-enable SDK `local-relay` only as needed; no shell or handwritten websocket fixtures.

**Target Platform**: Native Windows and Linux; financial regtest remains Linux/WSL and is not exercised by this read-only pilot.

**Project Type**: CLI binary plus existing `mostro_client` library; no direct MCP library linking.

**Performance Goals**: Seven-day query, limit 50 per relay, cap 10,000 unique buffered events. Network budgets: 10 seconds connection, 15 seconds collection, 1 second cleanup; these do not bound OS scheduling or stdout latency.

**Constraints**: No trade publication, key reservation, DB/identity access or prompts. Ephemeral in-memory NIP-42 authentication is allowed, not a trade. One JSON document for controlled outcomes. Safe diagnostics only; new/migrated Rust files at most 400 production lines with top imports.

**Scale/Scope**: Two machine-capable commands (`order.list`, `schema`), plus early JSON rejection for legacy commands. No write executor, full command migration, MCP upgrade, pagination or all-statuses filter.

## Constitution Check

The [constitution](../../.specify/memory/constitution.md) governs implementation.
Research-entry review passed for documentation-only work: no code changes, real
identities, trades, Git mutations or optional hooks were needed.

| Principle | Design compliance and implementation gate |
| --- | --- |
| I: Imports | Explicit top imports/aliases in extracted and new modules; inspect user edits before changing legacy files. |
| II: Size | Split the roughly 810-line production section of `cli.rs` before adding behavior. Inventory other violations without growing them; every migrated file meets the limit. |
| III: Reuse | Reuse decoder/reducer, filters, TLS and table helpers. Justify schemars for generation; relay fixture features are dev-only. |
| IV: Boundaries | Reads enter `fetch_msg`; commands cannot access the client. Pure renderers return output to `main`. No wholesale DB/core/context serialization. |
| V: Evidence | Retain earlier regressions. Characterize selection, output recovery and completion before implementation; proposed tests are not passing evidence. |
| VI: Safety | No trade mutation/retry, identity storage or stdin access. Isolated tests cover secrets/effects in both modes. |
| Workflow | Stay on `develop`; no stage/commit/branch/remote operations or live-node tests. |

**Post-design assessment**: No constitutional exception is proposed. Implementation
still requires the roadmap baseline below. Legacy repository compliance is not
claimed by this design review.

## Prerequisites And Sequence

The cross-repository roadmap is `plan-aiReadyMostroCli.md` (beside the clones, outside both repositories). Complete its
Phase 1 gate before pilot implementation: command/state baseline, closed-stdin
confirmation, take-with-invoice post-accept-save and correlated-wait regressions,
and legacy size/import inventory. Record CLI/core/daemon compatibility and
platform/toolchain checks separately. Preserve prior passing persistence tests;
they are not pilot acceptance.

1. Characterize selection/defaults and reproduce empty-versus-failed reads with isolated fixtures. Prove output recovery against adversarial Clap inputs.
2. Extract CLI parsing, legacy dispatch and initialization; add fallible parsing and pre-context JSON rejection.
3. Add public configuration/private session and EOSE-aware `fetch_msg`, preserving selection without DB dependencies.
4. Add DTOs, typed operation, pure renderers and binary output; replace `listorders` with `order list`.
5. Generate discovery from Clap/DTOs, add drift fixtures and process scenarios, and update migration docs.
6. Run focused/required gates and native Windows/Linux acceptance; record actual evidence.

The [task list](tasks.md) assigns test-first dependencies and carries these
baseline prerequisites explicitly. Setup T001-T003 and baseline checkpoints
T004-T010 are recorded in [the test summary](../../tests/TEST_SUMMARY.md#t010-baseline-gate-decision-2026-10-20);
the baseline gate is closed with recorded exceptions. Foundations T011-T019 are
recorded in [the test summary](../../tests/TEST_SUMMARY.md#t019-foundation-gate-2026-10-22),
and US1 test tasks T020 and T021 in [their](../../tests/TEST_SUMMARY.md#t020-order-list-contract-tests-2026-10-22)
[sections](../../tests/TEST_SUMMARY.md#t021-collector-contract-tests-2026-10-23),
and DTO mapping T022-T024 in [its section](../../tests/TEST_SUMMARY.md#t022-t024-order-list-dto-mapping-2026-10-23),
and public configuration T025 in [its section](../../tests/TEST_SUMMARY.md#t025-public-configuration-and-session-2026-10-23),
the collector T026 in [its section](../../tests/TEST_SUMMARY.md#t026-collector-2026-10-23),
US1 T027-T033 in [their sections](../../tests/TEST_SUMMARY.md#t027-order-reducer-extraction-2026-10-26)
ending at [the milestone](../../tests/TEST_SUMMARY.md#t033-us1-milestone-internal-2026-10-26),
US2 T034-T036 in [its section](../../tests/TEST_SUMMARY.md#t034-t036-us2-human-presentation-2026-10-26)
T037 in [its section](../../tests/TEST_SUMMARY.md#t037-output-ownership-and-failure-behavior-2026-10-27)
and the US2 acceptance T038 in [its section](../../tests/TEST_SUMMARY.md#t038-us2-acceptance-2026-10-27)
T039 in [its section](../../tests/TEST_SUMMARY.md#t039-us3-parser-and-registry-tests-2026-10-27)
T040 in [its section](../../tests/TEST_SUMMARY.md#t040-us3-process-tests-2026-10-27)
T041 in [its section](../../tests/TEST_SUMMARY.md#t041-discovery-dtos-2026-10-27)
T042 in [its section](../../tests/TEST_SUMMARY.md#t042-generated-schemas-2026-10-27)
T043 in [its section](../../tests/TEST_SUMMARY.md#t043-offline-schema-dispatch-2026-10-28)
T044 in [its section](../../tests/TEST_SUMMARY.md#t044-safe-error-mapping-2026-10-28)
T045 in [its section](../../tests/TEST_SUMMARY.md#t045-schema-fixtures-and-drift-checks-2026-10-28)
T046 in [its section](../../tests/TEST_SUMMARY.md#t046-full-process-matrix-2026-10-28)
the US3 acceptance T047 in [its section](../../tests/TEST_SUMMARY.md#t047-us3-acceptance-2026-10-28)
T048-T049 in [their section](../../tests/TEST_SUMMARY.md#t048-t049-documentation-2026-10-28)
T050 in [its section](../../tests/TEST_SUMMARY.md#t050-source-boundary-checks-2026-10-28)
and T051 in [its section](../../tests/TEST_SUMMARY.md#t051-ci-wiring-2026-10-28);
T052-T055 remain pending.

## Execution Design

### Invocation Boundary

Use `args_os()` and fallible Clap parsing. `--output` is a global value enum,
default human regardless of TTY. On parse failure recover intent through a small
Clap-metadata-led bootstrap, not substring scans or token removal. Respect scopes,
arities, attached/hyphen values and `--`. Follow the malformed-argv decision table
in [the CLI contract](contracts/cli.md#streams-and-exit-codes): invalid/missing/
duplicate output selection uses safe stderr; unrelated argument errors preserve
clear JSON intent, even with unresolved command identity. Helper limitations do
not create fallback exceptions. Help/version remain text exceptions.

Dispatch: parse, capability guard, offline schema, public listing, legacy human.
Validate filters/config before networking. Schema and rejection precede
`run_rpc`, `init_context`, environment mutation and identity helpers. Preserve
legacy human RPC-before-context routing.

Replace `listorders --kind` with `order list --side`, with no alias. Rename root
`--mostropubkey` to `--mostro-pubkey` as agreed in the roadmap, retaining its Rust
field if useful. Other legacy command spellings/options remain until migration.
Only `--output` must newly work at every depth; root configuration options stay
before the command. Do not globalize colliding short flags.

### Public Read Boundary

Resolve only `MOSTRO_PUBKEY` and `RELAYS`, CLI over environment, without `set_var`.
Preserve public-key parsing, comma splitting, trimming/dropping empty relay
entries and URL validation. Dedup canonical relay URLs before counting EOSE.
Do not read admin credentials, `MCLI_DIR`, identity keys or DM protocol metadata.

Private `msg::Session` owns the client/public configuration. Reuse connection/TLS
and ephemeral authentication; retain legacy Context until later migration.
The list wrapper calls only `fetch_msg`. Reuse kind 38383, author, `#z=order`,
seven-day `since`, limit 50 and no `until`.

`msg` is a public module with a narrow surface: `PublicReadConfig`,
`ReadLimits`, `fetch_msg` and its result types. No client, relay or session
handle is exported, so process tests can drive the collector without a raw-client
accessor. The caller builds the filter, which fixes the query clock.
`ReadLimits` carries the connect/read/cleanup budgets, the unique-event cap and
the per-relay notification capacity; its default is the production 10/15/1
seconds, 10,000 events and the SDK's 2,048 notifications. Tests shrink these
instead of waiting on production budgets.

Arm notifications before subscribing; track relay/subscription IDs. Accept only
matching, SDK-verified events satisfying the requested filter; dedup event IDs
and feed the reducer in the SDK event-set order (timestamp descending, event ID
ascending), not arrival order. Freeze each relay's snapshot at first EOSE. All relays
must complete: CLOSED, pre-EOSE disconnect, notification lag, deadline,
connection failure or event-cap overflow returns error with no partial results.
Stream termination or fetch `Ok` is not completion evidence. No retries.

Always unsubscribe/disconnect with bounded cleanup. Cleanup timeout after valid
EOSE adds a safe warning; error cleanup never replaces the original error.
Cancellation releases resources but cannot guarantee final stdout.

### Outcomes, Rendering And Discovery

Owned typed outcomes include normalized filters and node public key for human
headings. Amounts use decimal strings; the data model/contract define units,
timestamps, nullability, errors and metadata. Reads never produce uncertain.

Move decoding/reduction into the receive boundary and table construction into
`parse_human`. Retain delegating Rust helpers for existing callers, not CLI
aliases. Renderers do no networking, persistence, logging or exit. Serialize
fully before `main` writes one document plus newline; write failure exits nonzero
without appending another document. Legacy human handlers still print temporarily;
whole-CLI output ownership is not yet migrated.

Allow-list diagnostics for migrated human/JSON paths, including verbose mode.
Suppress raw SDK/debug logging rather than redact arbitrary secret-bearing text.
Escape terminal controls in human public text; JSON uses normal escaping.

Human presentation of `schema` is pretty-printed raw discovery JSON, as the
roadmap prescribes. The output mode still defaults to human. Explicit
`--output json` wraps that same data in an outcome. Generate input metadata from Clap/shared
validators and schemas from DTOs with schemars. Schema documents are JSON objects
to avoid recursively generating their schema. Advertise implemented coverage
only; planning contracts are not generated production fixtures.

## Project Structure

### Documentation (This Feature)

```text
specs/001-order-list-json/
  spec.md
  plan.md
  tasks.md
  research.md
  data-model.md
  quickstart.md
  contracts/cli.md
  contracts/examples.json
  checklists/requirements.md
```

The task list records T001-T051 as complete; its checkboxes track
execution, not design approval or completion of the pilot.

### Planned Source Changes (Repository Root)

```text
src/
  main.rs                     # migrated stdout/exit boundary
  lib.rs
  cli.rs                      # small facade/re-exports
  cli/args.rs                 # root args/output/range validation
  cli/commands.rs             # enum and nested order args
  cli/invocation.rs           # safe parse errors/output recovery
  cli/context.rs              # extracted legacy initialization
  cli/legacy.rs               # extracted legacy human dispatch
  cli/schema.rs               # Clap/DTO discovery
  commands/mod.rs
  commands/order/mod.rs
  commands/order/list.rs      # typed operation, no raw client
  msg/mod.rs                  # private Session/public config
  msg/receive_msg.rs          # bounded subscription/fetch
  msg/orders.rs               # private decoder/reducer/filter helper
  outcome.rs                 # envelope/errors/metadata
  outcome/order.rs            # public DTOs/normalized input
  parse_machine.rs
  parse_human/mod.rs
  parse_human/table.rs
  parse_human/order.rs
tests/
  cli_functions.rs            # reuse argument tests
  parser_orders.rs            # reuse selection fixtures
  integration_tests.rs        # reuse filter tests
  order_list_process.rs       # one portable process/relay test home, incl. collector contract
```

**Structure decision**: These are responsibilities, not mandatory empty files.
Keep extracted files below 400 production lines; split the command enum by
family if needed. Do not build future send/write modules or a generic service
framework. Subscriptions/fetch stay in `receive_msg`; `msg/orders` is private.
Retain one implementation per behavior through delegating legacy helpers.
Untouched oversized DB/DM modules remain inventoried migration debt.

Update current architecture, command/migration and test docs as code lands.
Check generated references/schemas for drift in CI; share test-only helpers.

## Verification Strategy

See [quickstart.md](quickstart.md). FR-001/008 map to selection/default/encoding
fixtures; FR-002/004/005 to parsing, streams and exits; FR-003/009 to envelope/schema
tests; FR-006/007 to process effects, discovery and capability guards. SC-001
through SC-005 all need runtime evidence before completion.

Run touched tests first, then fmt, locked all-target/all-feature Clippy with
warnings denied, locked tests and build. Run native Windows/Linux process checks
separately and check MSRV without hiding failures. Privacy/import/size/schema
drift checks are feature gates. Planning validates metadata, links, examples and
completeness, not runtime acceptance.

## Complexity Tracking

No constitutional violation is proposed. Schemars replaces duplicated handwritten
schemas; explicit collection is needed because convenience fetching discards
failure/completion distinctions. Both choices have focused implementation gates.
