# Implementation Plan: Order Info For Humans And Agents

**Branch**: `develop` | **Date**: 2026-10-28 | **Spec**: [spec.md](spec.md)

**Feature directory**: `002-order-info-json`, not a Git branch.

**Status**: Proposed design; no implementation and no runtime evidence. Behavior against a real daemon is unverified.

## Summary

Deliver `order info --order UUID...` (replacing `ordersinfo`) as the first correlated request and reply on the
migrated path: one signed request published through a new private `msg` exchange, the reply selected by request
identifier, typed outcome rendered by the existing pure renderers, offline discovery and drift-checked schema fixtures. The
identity is read from `mcli.db` without creating, migrating or writing anything. The pilot's outcome, registry, schema, fixture and
boundary machinery is extended, not redesigned. Evidence and decisions: [research.md](research.md).

## Technical Context

**Language/Version**: Rust 2021, declared minimum 1.94, pinned development toolchain 1.97.0 (as the pilot).

**Primary Dependencies**: existing only: Clap, `mostro-core` =0.17.3 (`wrap_message_with`, `unwrap_incoming`, `Message`, `Payload`, `CantDoReason`),
nostr-sdk 0.45.x, sqlx (read-only), serde, schemars, tokio, uuid, chrono. No new dependency is planned. Test side: the existing
`nostr-sdk` `local-relay` fixtures and `tests/common/schema_check.rs`.

**Storage**: reads the existing local `mcli.db` (`users` row) read-only; no write, creation, migration, permission change or new model.

**Testing**: unit tests beside the code; `cli_functions` (grammar and registry); `parser_orders` (typed data and rendering); a new
`tests/order_info_process.rs` running the binary against a scripted fake daemon on the loopback relay; `integration_tests`
(`schema_fixtures`, `source_boundaries`, `ci_wiring`) updated. Regtest is a separate, explicitly authorized probe.

**Target Platform**: native Linux and Windows for the process tests (Windows run deferred to a native machine, as for the pilot);
regtest on Linux or WSL.

**Project Type**: CLI binary plus the `mostro_client` library; no MCP linking or change.

**Performance Goals**: connect 10 s, node-info probe 3 s (best effort), reply 15 s, cleanup 1 s; `ExchangeLimits` lets tests shrink them.

**Constraints**: exactly one published event; no retry or splitting; no prompt; one JSON document for controlled outcomes; files at most 400
production lines with top-of-file imports; no new direct send, fetch or subscribe outside `msg`.

**Scale/Scope**: one command, eight error codes and two warnings added to the shared enums but scoped per command, one optional `error.reason`.
Out of scope: `order new`, `order take`, other write commands, MCP changes, pagination, streaming.

## Constitution Check

| Principle | Design compliance and implementation gate |
| --- | --- |
| I: Imports | New and touched files import at the top; the boundary test already rejects local and late imports in production code |
| II: Size | `cli.rs` (316 lines) is split first into `cli/migrated.rs`; new files planned below stay under 400 each; no growth of the four frozen legacy files (`db.rs` shrinks by delegating its key derivation) |
| III: Reuse | Reuse `select_reply_event`/`ExpectedReply`, `wrap_message_with`/`unwrap_incoming`, `PublicReadConfig`, `Session`, the info-tag parser, the outcome envelope, the table helpers and the schema generator. No new dependency. A second copy of any correlation or derivation rule is a defect |
| IV: Boundaries | The wrapper returns a typed outcome; renderers are pure; all publication and reception are in `msg`, whose client stays private; subscribe before publish; explicit key scope (identity key), correlation, deadline and cleanup; machine mode is one versioned document with diagnostics on stderr |
| V: Evidence | The legacy `ordersinfo` behavior is characterized first. Defects get a failing check before the repair. The scripted daemon encodes the source-derived behavior; only a regtest probe, recorded separately, supports a claim about a daemon. Nothing is marked complete by compilation or documentation |
| VI: Safety | A read: one request, no trade key, no trade-index change, no write, no retry. The identity file is opened read-only and never created. No mnemonic or key in output, logs or fixtures committed; node text is untrusted and escaped. Public-node use needs separate authorization; regtest uses regtest identities only |
| Workflow | Stay on `develop`; no staging, commits, branches, pushes or live-node tests without authorization |

**Post-design assessment**: no constitutional exception is proposed. The new `identity.rs` is a bounded new file, not a
generic framework. Legacy repository compliance is not claimed.

## Prerequisites And Sequence

The pilot's native Windows run is not a prerequisite; its results will be recorded separately. Steps are
dependency-ordered and test-first; the task list will carry them.

1. Characterize `ordersinfo` (arguments, message built, reply handling) and capture the daemon's reply shapes as fixtures from the source-derived table in [research.md](research.md). Extract `cli/migrated.rs` from `cli.rs`, behavior-preserving, tests rerun.
2. Add `identity.rs` with the read-only loader and typed errors; make `db.rs` delegate the derivation. Tests first: absent, unreadable, valid store; byte-identical file; nothing created.
3. Add the outcome pieces: codes, warnings, `ExecutionMeta` request constructors, the optional `error.reason`, `JsonSupport::Request`, `OrderInfoData` and its mapping from a reply; keep `order.list` and `schema` documents equal (the existing drift tests guard this).
4. Add the `msg` exchange: node-info probe, `send_msg.rs`, `reply.rs`, `exchange.rs`, `ExchangeLimits`, with the scripted fake daemon fixture in `tests/common`.
5. Add `commands/order/info.rs`, the `OrderCommands::Info` grammar, the registry move, the dispatch, `parse_human/order_info.rs`; remove `Commands::OrdersInfo`, `cli/orders_info.rs` and the legacy arm.
6. Generate the schemas (command-scoped code sets, `reason`), regenerate `docs/schemas`, add `order-info-v1.json`, update the boundary-test frozen counts deliberately, CI step names, docs and README migration notes.
7. Run the focused and required gates; then, only with authorization, the regtest probe; record everything in `tests/TEST_SUMMARY.md`.

## Execution Design

### Invocation Boundary

`order info` is `OrderCommands::Info { orders: Vec<Uuid> }` with a required, repeatable `--order`. UUIDs are canonicalised to lowercase,
deduplicated in order, and validated by Clap, so a bad id is `invalid_arguments` with the resolved identity `order.info` before any effect.
Dispatch runs after discovery and listing and before the generic JSON guard, with no `init_context`, no `set_var`, no logger from user
input and no stdin. Other commands keep the early `output_not_supported` rejection.

### Identity And Configuration

`cli/migrated.rs` resolves configuration (`PublicReadConfig::resolve` plus `--pow` and `--transport`, CLI over environment, no fallback
from an invalid CLI value), then calls `identity::load()`. `commands::order::info` receives the keys as an argument and never touches
storage, so tests inject generated keys. Failures map to `identity_missing` or `identity_unreadable` before any network access.

### Exchange Boundary

`msg::exchange` connects once, probes the node info, applies the transport and work rules, arms a subscription (kind 14, tagged to the
identity key, authored by the node, limit 0), publishes with `send_msg`, waits with `reply` using the existing correlation, and cleans up in
bounded time. A relay accepting the event counts as published; no relay accepting is `request_rejected`. A post-timeout probe turns a work shortfall into
`pow_required`. The decoded reply is returned to the wrapper, which maps `Orders`, `CantDo(not_found)` (empty success), other `CantDo`
(`node_refused` with the reason) and anything else (`reply_malformed`). Local effects: none.

### Outcome, Rendering And Discovery

Typed outcomes only; `parse_machine::emit` and a pure `parse_human/order_info.rs` render the same data. The output schema is generated from the
DTOs with the stage and code constraints per branch; shared `$defs` for the code and warning enums list the frozen pilot set plus the command's own,
so existing documents are unchanged. The input schema comes from Clap. `docs/schemas/order-info-v1.json` is a new generated fixture with the
same drift checks; `discovery-v1.json` gains the `order.info` descriptor (the legacy entry becomes a machine entry).

## Project Structure

### Documentation (This Feature)

```text
specs/002-order-info-json/
  spec.md  plan.md  tasks.md  research.md  data-model.md  quickstart.md
  contracts/cli.md
  checklists/requirements.md
```

[tasks.md](tasks.md) holds the 44 dependency-ordered tasks. Proposed outcome examples are added as `contracts/examples.json` by task T034, as in the pilot.

### Planned Source Changes (Repository Root)

```text
src/
  identity.rs                   # read-only identity loader, shared derivation
  cli/migrated.rs               # extracted dispatch of listing, discovery, info
  cli/commands.rs               # OrderCommands::Info; OrdersInfo removed
  cli/registry.rs               # order.info -> ["order","info"], JsonSupport::Request
  cli/schema.rs                 # command_schemas for order.info
  cli/orders_info.rs            # removed
  commands/order/info.rs        # typed operation: request, map reply
  msg/exchange.rs               # public exchange, ExchangeLimits, types
  msg/send_msg.rs               # private door: wrap and publish
  msg/reply.rs                  # private door: correlated reply
  msg/node_info.rs              # kind-38385 probe: protocol, work
  outcome.rs, outcome/info.rs   # codes, warnings, stage constructors, reason; DTOs
  outcome/schema.rs             # command-scoped code sets, reason property
  parse_human/order_info.rs     # pure renderer
tests/
  order_info_process.rs         # binary against the scripted daemon
  common/fake_mostro.rs         # scripted daemon and identity-store seeding
  cli_functions.rs, parser_orders.rs, integration_tests.rs   # extended
docs/schemas/order-info-v1.json # generated, drift-checked
```

**Structure decision**: these are responsibilities, not mandatory empty files; the `msg` doors stay private. `db.rs` is only allowed to shrink.
Update architecture, command reference, README migration notes and test docs as code lands, and regenerate fixtures with the documented commands.

## Verification Strategy

See [quickstart.md](quickstart.md). FR-001/001a map to the id, subset, not-found and extras cases; FR-002/003 to the single-request, correlation and
identity-store checks; FR-004/006 to envelope, parity and output-position tests; FR-005/007 to stage and error matrices; FR-008/009 to schema, drift and
boundary tests; FR-010 to sentinel and escape tests; FR-011 to the written contract. SC-001 to SC-006 need runtime evidence before completion, and no claim
about a daemon is made without the regtest probe.

## Risks And Open Items

- **Daemon version**: the behavior table is from v0.20.1 source; regtest pins v0.19.3. The probe decides what is claimed.
- **Work rule**: whether the identity key is "first contact" for the daemon is unknown; the plan checks the base `pow` before sending and both after a timeout.
- **MCP**: `get_order` breaks against this CLI until migrated; documented in the contract and README.
- **Windows**: not run; follows the pilot's follow-up.
- **Reply size**: the node bounds what it returns; the command ignores unrequested orders, so output is bounded by the request.
- **Frozen boundary counts** change deliberately: `.send_event(` gains `msg/send_msg.rs`, `.subscribe(` and `.fetch_events(` gain their `msg` owners.

## Complexity Tracking

| Addition | Why it is needed | Simpler option rejected |
| --- | --- | --- |
| New `identity.rs` | A read must not use `connect()` or `get_mcli_path` (they create, chmod, migrate) and `db.rs` is frozen | Flags on the legacy functions keep a create path next to the read path |
| Eight codes, two warnings, optional `error.reason` | Agents must tell a missing identity, a refusal and a timeout apart; the node's reason set evolves | One generic error, or a reason inside the fixed message |
| Command-scoped `$defs` code sets | Keeps `order.list` and `schema` documents unchanged | A new schema version, or changing the pilot fixtures |
| `msg::exchange` instead of two public functions | The client stays private and arming before publishing needs one connection | Exporting a session handle |
