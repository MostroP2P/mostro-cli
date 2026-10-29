# Feature Specification: Order Listing For Humans And Agents

**Feature**: `001-order-list-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-10-16

**Status**: Implemented and verified on Linux with loopback relays (2026-10-28); SC-005 (Windows) passed on a GitHub-hosted native Windows runner on 2026-10-29. See [the plan](plan.md) and `tests/README.md`

**Input**: Pilot from the agreed AI-ready CLI roadmap: canonical `order list`,
human/JSON output, structured errors, and offline schema discovery.

## Clarifications

### Session 2026-10-16

- Q: When may malformed arguments produce stderr instead of JSON? -> A: A clear JSON selection still requires a JSON error for unrelated argument failures. Missing, invalid or duplicate output declarations use the stderr fallback; option-like value text and tokens after `--` do not select output. Help/version and process-failure exceptions remain documented separately.

## User Scenarios & Testing

### User Story 1 - Read Orders Without Parsing Tables (Priority: P1)

An automation client requests the public order book and reads structured orders
without interpreting human tables or accidentally initiating a trade.

**Why this priority**: This is the smallest safe command that proves the machine
output contract before financial mutations are migrated.

**Independent Test**: Supply a controlled public-book snapshot, request JSON,
and verify the exact selected orders, one complete document, and no trade effects.

**Acceptance Scenarios**:

1. **Given** a book with matching and nonmatching orders, **when** a client runs `order list --output json` with side/currency/status filters, **then** it receives only the matching latest order states in one structured result.
2. **Given** no matching orders, **when** listing finishes, **then** it succeeds with an empty orders collection rather than an error or empty stdout.
3. **Given** diagnostics or startup notices, **when** JSON is selected, **then** stdout still contains exactly one JSON document and diagnostics use stderr.

### User Story 2 - Keep Readable Human Results (Priority: P2)

A person lists orders without choosing an output format and sees the familiar
fields and readable tables, using the new command spelling.

**Why this priority**: Machine support must not remove the useful human interface.

**Independent Test**: Render the same controlled book in both modes and compare
order identities, statuses, amounts, currency, and payment methods.

**Acceptance Scenarios**:

1. **Given** an ordinary terminal invocation without `--output`, **when** `order list` runs, **then** human output is selected regardless of whether stdout is redirected.
2. **Given** the same book and filters, **when** both formats run, **then** they describe the same selected orders and do not change stored order or trade-index state.

### User Story 3 - Discover Inputs And Handle Failures (Priority: P2)

An agent discovers the supported machine interface offline and distinguishes
invalid inputs, configuration problems, and failed reads from an empty book.

**Why this priority**: Agents need a reliable contract before invoking commands.

**Independent Test**: Request schema discovery without node settings or identity
files, then exercise invalid arguments and failed reads with controlled inputs.

**Acceptance Scenarios**:

1. **Given** no configured node or identity, **when** `schema` is requested, **then** the CLI describes the pilot's inputs, results, errors, and supported schema version without connecting or creating state.
2. **Given** an invalid side/status or missing configuration, **when** JSON output is requested, **then** the CLI returns a structured error and a nonzero exit without publishing a trade.
3. **Given** a read timeout, **when** listing terminates, **then** it reports a read error, not an empty success or an uncertain financial mutation.
4. **Given** an unmigrated command, **when** JSON is requested, **then** it returns `output_not_supported` before command execution or context initialization.
5. **Given** exactly one valid JSON output declaration in an option position, **when** another argument or the command name is invalid, **then** it returns a JSON `invalid_arguments` error with exit 2, using a null command name when unresolved.
6. **Given** a missing or invalid output value, or duplicate output declarations even with identical values, **when** parsing fails without a help/version result, **then** stdout is empty and a safe diagnostic uses stderr with exit 2.
7. **Given** text resembling an output flag inside an argument value or after `--`, **when** output selection is determined, **then** that text neither selects JSON nor overrides a real output declaration.

### Edge Cases

- Global output flags before/after the command, `--output=json`, and `--` termination.
- Closed stdin, redirected output, verbose mode, and fresh/existing local storage.
- Duplicate order events, multiple updates, malformed events, and nonmatching filters.
- Empty books versus failed relay reads; bounded snapshots versus complete history.
- Missing argument values and unknown output modes; documented help/version exceptions.
- Secret-shaped fixture values must not escape through ordinary output or diagnostics.

## Requirements

### Functional Requirements

- **FR-001**: Provide `order list` with `--side`, `--currency`, and `--status` filters. Preserve existing listing defaults, event selection, and latest-state reduction. Remove the replaced `listorders` spelling and the root `--mostropubkey` spelling (replaced by `--mostro-pubkey`) without a compatibility adapter.
- **FR-002**: Support global `--output human|json`, defaulting to human regardless of terminal detection, in both flag positions. Listing must never prompt.
- **FR-003**: JSON must contain one versioned outcome with canonical command `order.list`, status, data, structured error, warnings, and metadata; success contains an orders collection even when empty.
- **FR-004**: Both renderings must describe the same selected orders. Diagnostic output must not contaminate the JSON document, including during startup and verbose execution.
- **FR-005**: Validation/configuration/read failures must have stable error codes and nonzero exits. A read-only timeout is an error. A clear JSON selection requires a JSON error even when other arguments or the command name are invalid. Missing, invalid or duplicate output declarations use empty stdout and safe stderr with exit 2; option-like value text and tokens after `--` are not declarations. Follow the [CLI contract](contracts/cli.md#streams-and-exit-codes) decision table, including help/version and process-failure exceptions; externally killed processes cannot guarantee a final document.
- **FR-006**: Listing must not publish trade mutations, update stored orders, or advance the trade index. Ordinary responses and errors must not expose private keys, mnemonics, or administrative tokens.
- **FR-007**: Schema discovery must work offline without identity creation and truthfully advertise implemented JSON coverage, inputs, and result/error shapes. Unmigrated commands requesting JSON must fail before side effects.
- **FR-008**: Document field units, required versus nullable values, ordering, timestamps, identifier encodings, and lossless large-number handling. Identify snapshot limits without claiming complete relay history.
- **FR-009**: Document supported schema versions and compatibility independently of CLI package versions. Consumers may ignore additive optional fields but must detect incompatible versions.

### Key Entities

- **Public order**: The latest selected public state, with order identity, side, status, sats amount, fiat currency/amount or range, and payment method where provided.
- **Command outcome**: One operation result, its structured data/error, diagnostics-safe warnings, and metadata identifying applicable limits and execution state.
- **Command schema**: Discoverable input constraints and output/error definitions, including which commands currently support machine mode.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Every success, empty-book, controlled-failure, startup, and verbose scenario with valid JSON selection produces exactly one valid document without prompts or human-table fragments, subject only to the documented help/version and process-failure exceptions. Output-selection failures follow FR-005 instead; unrelated argument errors do not waive the JSON guarantee.
- **SC-002**: Human and machine results agree on all selected order identities and business fields across the fixture matrix; listing produces zero trade mutations and zero trade-index changes.
- **SC-003**: Offline discovery succeeds with no node configuration and creates no identity files; agents can determine pilot inputs and supported result versions without scraping help text.
- **SC-004**: All invalid-input and failed-read cases return a nonzero exit and a documented error; empty books remain successful results.
- **SC-005**: Native Windows and Linux process-level checks pass for the pilot. Ordinary outputs contain none of the planted secret sentinels used in tests.

## Assumptions

- This is a read-only pilot, not implementation of the complete CLI refactor or MCP migration.
- Existing Mostro public-book semantics remain authoritative; no new pagination or exhaustive-history guarantee is introduced.
- Trade workflows, peer chat, administrative commands, and secret export remain separate features; this pilot only provides their pre-execution unsupported-JSON guard.
- The constitution and cross-repository roadmap remain authoritative for governance and phase dependencies. Existing persistence fixes are prior work, not completed pilot tasks.
- Detailed data shapes, test design, and module changes belong in the technical plan; runtime acceptance criteria here remain unverified until implementation.
