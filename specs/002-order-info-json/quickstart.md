# Validation Guide: Order Info

**Status**: Implemented and verified on Linux with loopback relays and a scripted fake daemon (evidence in
`tests/TEST_SUMMARY.md`). The regtest probe below has not been run; no claim about a real daemon is made.

## Prerequisites

Work in mostro-cli on `develop`. Read the [plan](plan.md), [contract](contracts/cli.md) and
[data model](data-model.md), and keep the order-list pilot's tests and fixtures. Use the pinned toolchain and check
the declared minimum separately. The process suite runs on native Linux and Windows (the Windows run is a follow-up
on a native machine, as for the pilot); regtest runs on Linux or WSL only.

Fixtures use generated keys and isolated temporary directories. Children get an allow-listed environment,
sandboxed `HOME`, `USERPROFILE` and `MCLI_DIR`, a closed stdin, captured streams, a temporary working directory and a
40-second parent watchdog. A scripted fake daemon (in `tests/common`) runs in the test process on the loopback relay,
holding the node's generated keys; it records every request it decrypts and replies by script. The identity store is a
real SQLite file created by the test with a generated mnemonic. No public relay, no real identity, no live node.

## Focused Commands

After the slices land, from the CLI root:

```sh
cargo test --locked --test cli_functions order_info
cargo test --locked --test parser_orders order_info
cargo test --locked --test order_info_process
cargo test --locked --test integration_tests schema_fixtures source_boundaries ci_wiring
```

## Acceptance Matrix

| Scenario | Expected result | Requirements |
| --- | --- | --- |
| One id, node returns it | `ok`, one order with every field, `not_returned` empty, one request published, `published / accepted / not_applicable` | FR-002/004/005, SC-001/003 |
| Several ids, node returns a subset | `ok`, returned orders in request order, others in `not_returned`, one warning | FR-001a, SC-004 |
| Repeated id | requested once; appears once | FR-001 |
| Node answers `not_found` | the same success with no orders and every id in `not_returned` | FR-001a |
| Reply contains an unrequested order or the same order twice | only requested ids, each once | FR-001a, SC-004 |
| Unrelated kind 14, other request id, own echo, wrong author, malformed reply before the real one | the real reply decides the result | FR-003, SC-002 |
| Matching reply with a bad action or payload | `reply_malformed`, exit 1 | FR-007 |
| Node refuses (`too_many_requests`, `invalid_parameters`, others) | `node_refused` with `error.reason`, `published / rejected` | FR-007 |
| No reply | `reply_timeout` after 15 s, request published once, nothing retried | FR-003/005 |
| Timeout while the node advertises more work than configured | `pow_required`, published | FR-007 |
| Node advertises more work than configured before sending | `pow_required`, nothing published | FR-007 |
| Node advertises protocol 1, or `--transport gift-wrap` | `protocol_unsupported` or `configuration_invalid`, nothing published | FR-007 |
| No relay accepts the request event | `request_rejected`, stage per cause | FR-007 |
| Relay closes or drops the subscription while waiting | `relay_read_failed` | FR-007 |
| Relay unreachable | `relay_connection_failed`, nothing published | FR-007 |
| No data directory, no `mcli.db`, no user row | `identity_missing`; nothing created in the sandbox | FR-002, SC-003 |
| Unreadable store (not a database, no mnemonic) | `identity_unreadable`; file unchanged | FR-002 |
| Existing identity store | byte-identical before and after, in both modes | FR-002, SC-003 |
| Missing or invalid arguments, none, bad UUID, `ordersinfo`, `--orderids` | `invalid_arguments` exit 2 before any effect; JSON identity `order.info` where resolved | FR-001/007, SC-005 |
| Missing or invalid configuration | `configuration_missing` or `configuration_invalid`; nothing published | FR-007 |
| Output flag at every position, default and explicit human, redirected stdout | one document / the same orders in both modes | FR-004/006, SC-001/004 |
| Verbose, closed stdin, planted secret sentinels in arguments, environment, URLs and node text | allow-listed stderr only; no sentinel in any stream; control characters escaped in human text | FR-010, SC-006 |
| Many ids (hundreds) | one request, no cap in the CLI, every id returned or not returned, or the node's refusal | SC-001a |
| `schema` offline | `order.info` listed with generated input and output schemas; `order.list` and `schema` documents unchanged | FR-008, SC-005 |
| Other unmigrated commands with `--output json` | `output_not_supported` before any effect | FR-008 |
| Failing stdout or stderr writer | exit 1, no second document | FR-006 |
| Boundary and drift checks | no direct send, fetch or subscribe outside `msg`; fixtures current | FR-008/009 |

Assert exact document consumption with Serde's end-of-input check. Count the request events in the relay store:
exactly one event of kind 14 authored by the identity key and no other published event. Compare the identity
file's bytes before and after every run, and compare the sandbox listing.

## Offline Smoke Check

Only after implementation, with an empty environment:

```sh
mostro-cli order info --order 00000000-0000-4000-8000-000000000001 --output json
mostro-cli schema --output json | jq '.data.commands[] | select(.name=="order.info")'
```

The first fails with `configuration_missing` and exit 1 before touching anything; the second lists the command.
Automated tests, not an unisolated shell, establish that no state is read or written.

## Regtest Probe (Linux, separately authorized)

Unit and process fixtures encode the daemon's behavior from its source. Before the feature is called verified against
a daemon, run a controlled probe on the MCP repository's regtest network with the regtest identities only (never a public
node, never `devenv.sh clean` as setup): create one order as Alice, then `order info` with that id, with a foreign or
random id, with a mix, with an empty list rejected by the CLI, and with more ids than the node's limit. Record, per case,
the daemon's answer (`orders`, `not_found`, `too_many_requests`) and the daemon version; v0.19.3 is the pinned regtest
version and the source read for the plan is v0.20.1.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, fixture isolation, result and blockers in `tests/TEST_SUMMARY.md`. Native
Windows results are recorded separately and stay open until run on a native host.
