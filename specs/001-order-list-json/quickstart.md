# Validation Guide: Order-List JSON Pilot

**Status**: Planned acceptance procedure. Commands for new functionality are not
available until implementation; no scenario below is recorded as passed.

## Prerequisites

Work in mostro-cli on develop. Complete the roadmap Phase 1 baseline before
implementing this pilot. Read [plan](plan.md), [contract](contracts/cli.md) and
[data model](data-model.md). Keep prior persistence tests and user edits.
Use the pinned Rust toolchain; check declared MSRV separately. Run the process
suite on native Windows and Linux, not just cross-compile. No MCP or Docker is
required for pilot tests. Financial regtest remains a separate Linux/WSL gate.

Fixtures must use generated keys and isolated temporary directories. Child
processes use an allow-listed environment with only necessary OS variables,
fixture HOME/USERPROFILE/MCLI_DIR, fixture public configuration, closed stdin,
captured streams, temporary working directory and a 40-second parent watchdog.
Never inspect real identity files or inherit admin tokens. Seed synthetic existing
databases only; compare their bytes/state before/after. Also use an unusable
fixture MCLI_DIR path to prove public operations never need identity storage.

Use the SDK's loopback relay and injectable websocket transport. Fixtures bind
loopback on an OS-assigned port and signal readiness; no guessed unused port,
shell executable, manual websocket framing or sleep-based readiness. Script EOSE,
CLOSED, silent responses and disconnects through SDK fixture APIs. Shut down and
reap children/relays even when assertions fail. Do not query public relays.

## Focused Commands

After the corresponding implementation/test slices land, from the CLI root:

```sh
cargo test --locked --test cli_functions
cargo test --locked --test parser_orders
cargo test --locked --test integration_tests
cargo test --locked --test order_list_process
```

The first three targets already exist and will be extended; the process target
is planned. Do not count existing passing tests as proof of unimplemented cases.
Run newly added unit tests for the collector, pure renderers and discovery by
their module filter during each increment, before broad gates.

## Acceptance Matrix

| Scenario | Expected result | Requirements |
| --- | --- | --- |
| Mixed signed events, same-ID updates, latest state nonmatching | Exactly latest matching orders; no resurrected stale pending state | FR-001/004, SC-002 |
| Duplicate events and equal timestamps | Event dedup, same-ID lowest-event-ID tie; cross-order tie order not asserted | FR-001/008 |
| Side/status case, whitespace and empty currency | Case-insensitive side/status without trimming; uppercase exact currency; pending default; no all option | FR-001/005 |
| Invalid/missing tags, incomplete ranges, zero and signed i64 extremes | Existing decoder defaults; lossless strings and human market/range display | FR-004/008, SC-002 |
| Invalid calendar timestamp | Integer retained, RFC3339 null, one safe warning and human Invalid date | FR-008 |
| Empty relay with matching EOSE | Exit 0, one document, orders empty | FR-003/005, SC-001/004 |
| Nonempty events but no matching orders | Same successful empty representation | FR-001/003 |
| CLOSED after accepted REQ | Exit 1, relay_read_failed, data null | FR-005, SC-004 |
| Silent relay after REQ | Exit 1, read_timeout at read deadline, no partial data | FR-005, SC-004 |
| One relay completes, another fails or times out | Whole read fails; completed counts preserved, no partial success | FR-005/008 |
| Duplicate relay URL, unrelated subscription/event, event after EOSE | Dedup relay expectations; ignore unrelated/live-after-EOSE events | FR-001/008 |
| Excess unique events, lag or disconnect before EOSE | Bounded failure, never truncated success | FR-005/008 |
| Output flag at root/intermediate/leaf and equals form | One JSON object; no prompt/human fragments | FR-002/004, SC-001 |
| Default or explicit human, redirected stdout | Human listing retained; same selected fields as JSON | FR-002/004, SC-002 |
| Missing/invalid args with clear JSON intent, including unknown command names | Exit 2, JSON invalid_arguments, resolved or null command identity; no stderr-only fallback | FR-005, SC-001/004 |
| Flags/command names inside values, hyphen values, short clusters, variadic values, `--` | Bootstrap agrees with Clap grammar; no false JSON/command detection | FR-002/005 |
| Invalid/missing/duplicate output, including identical duplicate declarations | Exit 2, empty stdout, safe stderr; apply every [contract decision-table case](contracts/cli.md#streams-and-exit-codes) | FR-005 |
| Valid help/version with output json | Conventional text exception, exit 0 | FR-005 |
| Missing/invalid public config | Exit 1, configuration_missing/invalid, no connection/state | FR-005/006 |
| Registered legacy JSON, including RPC and secret-export paths | Exit 2, output_not_supported before initialization/RPC | FR-006/007 |
| Removed listorders/--mostropubkey spelling | Invocation failure before effects; no aliases | FR-001/005 |
| Offline schema with absent or invalid node config | Success; no network, prompt, identity access or files | FR-007, SC-003 |
| Verbose and fresh/existing/unusable identity locations | Same clean streams and no identity changes | FR-004/006, SC-001/002 |
| Planted secret sentinels in invalid args/config and untrusted diagnostics | No sentinel in stdout/stderr; no raw chains/URLs | FR-006, SC-005 |
| Serialization and failing output writer | Exit nonzero; no success claim or appended second document | FR-004/005 |
| Discovery, generated schemas and owned DTO examples | Compatible schema version, exact implemented coverage, no fixture drift | FR-003/007/009, SC-003 |

Assert exact document consumption with Serde's end-of-input check, not merely a
successful prefix parse. Compare business fields after applying documented human
formatting, including premium, range endpoints and timestamp formatting. Test
zero trade EVENT publications separately from allowed NIP-42 AUTH traffic.
Compare synthetic identity/order/trade-index state before and after both modes.

## Offline Manual Smoke Check

Only after implementation, these commands should need no node/identity:

```sh
cargo run --locked -- schema
cargo run --locked -- schema --output json
cargo run --locked -- order list --side invalid --output json
```

The first prints pretty raw discovery data; the second an ok envelope; the third
an invalid_arguments envelope and exits 2. Automated process tests, not an
unisolated local shell, establish that configuration/state were never accessed.
Use the loopback test harness for successful listing; do not substitute live-node
credentials or relay URLs into a manual smoke test.

## Required Gates And Evidence

After focused checks, completed code increments run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
```

Check the declared minimum separately with the installed Rust 1.94.0 toolchain:

```sh
cargo +1.94.0 check --locked --all-targets --all-features
```

Report missing toolchains, pre-existing failures and native-platform gaps; do not
raise the minimum or claim Linux coverage from Windows results. Run migrated-file
size/top-import checks, client-privacy/transport-boundary checks, generated
command/schema drift tests and JSON example conformance. No release is implied.

Record command, platform/toolchain, fixture isolation, outcome and unresolved
blockers in the implementation evidence. Planning validation checks metadata,
Markdown links and example syntax/invariants only. It is not execution evidence
for any row above. The earlier MCP Unix fixture blocker remains separate.
