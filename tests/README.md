# Mostro CLI Test Suite

This directory contains comprehensive unit and integration tests for the Mostro CLI application.

## Test Files

Counts are from `cargo test --locked --no-fail-fast` on Linux at the end of the
order-list pilot (428 tests, none ignored). Historical per-file narratives from
earlier refactors are in [TEST_SUMMARY.md](TEST_SUMMARY.md).

1. **`parser_dms.rs`** (19 tests): direct message parsing and display, payload handling, Mostro identification, error cases.
2. **`cli_functions.rs`** (67 tests): command logic and message creation; `output_intent` (output recovery from malformed argv and the contract decision table, including `schema`) and `registry` (the approved canonical names and JSON capability); `new_order_preflight_process` (closed stdin, Enter, rejection).
3. **`util_misc.rs`** (17 tests): path and string helpers.
4. **`parser_orders.rs`** (110 tests): order event parsing, filters and tables; `list_contract`, the `order.list` DTO, selection and human-rendering contract.
5. **`parser_disputes.rs`** (9 tests): dispute event parsing and display.
6. **`integration_tests.rs`** (33 tests): context and filter integration (14); `schema_fixtures` (8: the checked-in `docs/schemas` files against the generated ones, proposed examples, mutations); `source_boundaries` (7: size, imports, transport ownership, no-I/O modules); `ci_wiring` (4: the workflow steps and jobs).
7. **`order_list_process.rs`** (46 tests): runs the real `mostro-cli` binary against in-process loopback relays (OS-assigned ports, generated keys, allow-listed environment, sandboxed `HOME`/`MCLI_DIR`/working directory, closed stdin, 40 s watchdog). Covers listing in both modes, read failures, the event cap, configuration errors, every legacy JSON rejection, offline `schema`, secret sentinels, unwritable streams and no-effects.
8. **`common/schema_check.rs`**: the small JSON Schema checker shared with the `outcome::schema` unit tests (not a test target).

The library also carries 124 unit tests and 3 doctests (`msg` privacy examples).

## Running Tests

### Run all tests
```bash
cargo test
```

### Run tests with output
```bash
cargo test -- --nocapture
```

### Run specific test file
```bash
cargo test --test parser_dms
cargo test --test cli_functions
cargo test --test util_misc
```

### Run a specific test
```bash
cargo test test_orders_info_empty_order_ids
```

### Run tests in parallel (default)
```bash
cargo test -- --test-threads=4
```

### Run tests serially
```bash
cargo test -- --test-threads=1
```

## Test Coverage

**Total Tests:** 78
- Unit Tests: 75 (97%)
- Integration Tests: 3 (3%)
- Async Tests: 16 (21%)
- Sync Tests: 62 (79%)

## Key Areas Tested

### 1. New Features
- ✅ `orders_info` command (5 tests)
- ✅ Enhanced `restore` command with response handling (2 tests)
- ✅ Table-based message display (16 tests)
- ✅ Colored output and icons (covered in display tests)

### 2. Modified Features
- ✅ Enhanced dispute handling (9 tests)
- ✅ Improved order display (11 tests)
- ✅ Rating system validation (3 tests)

### 3. Edge Cases
- ✅ Empty collections
- ✅ Invalid inputs
- ✅ Boundary conditions
- ✅ Data integrity

## Test Patterns

### Message Creation
```rust
let message = Message::new_order(
    Some(order_id),
    Some(request_id),
    Some(trade_index),
    Action::Orders,
    Some(payload),
);
```

### Async Testing
```rust
#[tokio::test]
async fn test_name() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Payload Validation
```rust
match payload {
    Payload::Expected(data) => {
        assert_eq!(data, expected);
    }
    _ => panic!("Unexpected payload type"),
}
```

## Best Practices

1. **Descriptive Names** - Test names clearly describe what is being tested
2. **AAA Pattern** - Arrange, Act, Assert structure
3. **Independence** - Tests don't depend on each other
4. **Fast Execution** - No network calls or heavy I/O
5. **Deterministic** - Consistent results across runs

## Contributing

When adding new tests:

1. Follow existing naming conventions
2. Use appropriate test attributes (`#[test]` or `#[tokio::test]`)
3. Test happy paths, edge cases, and error conditions
4. Keep tests focused and simple
5. Add documentation for complex test logic

## CI/CD

These tests are automatically run in CI/CD pipelines. All tests must pass before code can be merged.

## Documentation

For detailed test documentation, see [`TEST_SUMMARY.md`](TEST_SUMMARY.md) in this directory.

## Order-List Pilot: Requirement Evidence

Reconciliation of the [specification](../specs/001-order-list-json/spec.md) with
the evidence in [TEST_SUMMARY.md](TEST_SUMMARY.md) (T052-T054), as of 2026-10-28.
**Automated verification on Linux with loopback relays only**; no live node, no
public relay and no Windows run. Contracts: [CLI](../specs/001-order-list-json/contracts/cli.md),
[data model](../specs/001-order-list-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json) and [listing](../docs/schemas/order-list-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 `order list` filters, defaults, selection, removed spellings | `list_contract`, `order_list_binary_selects_rows_in_both_modes`, `output_intent::every_side_and_status_spelling_parses_and_removed_spellings_fail` | Verified on Linux |
| FR-002 global `--output`, human default, both positions, no prompts | `every_output_position_returns_the_same_listing`, `output_intent`, closed stdin in every process run | Verified on Linux; default-by-TTY only with piped stdout (the code has no terminal detection) |
| FR-003 one versioned outcome, empty list on empty success | `outcome::tests`, `outcome::schema::tests`, `order_list_binary_empty_results_are_successes`, `schema_fixtures` | Verified on Linux |
| FR-004 same orders in both renderings, clean streams | `order_list_human_and_json_describe_the_same_book`, `list_contract::human_rows_carry_the_json_business_fields`, `order_list_diagnostics_are_allow_listed_in_verbose_mode` | Verified on Linux |
| FR-005 stable codes and exits, timeout is an error, JSON intent kept, documented exceptions | decision table (`output_intent::contract_decision_table_rows`), `read_failures_are_stable_in_every_mode_without_retries`, `configuration_errors_are_stable_and_leak_nothing`, `an_unwritable_stdout_exits_1_with_one_fixed_line` | Verified on Linux, except: `internal_error` has no process-level case (unit only); a crash or external kill cannot guarantee a document, by definition |
| FR-006 no trade mutation, state change or secret exposure | no-effects assertions (empty sandboxes, byte-identical `mcli.db`, unchanged relay stores, zero connections on rejection), sentinel sweeps, `source_boundaries` | Verified on Linux with loopback relays; NIP-42 AUTH is never exercised by them |
| FR-007 offline truthful discovery; unmigrated JSON fails early | `schema_runs_offline_in_both_modes`, `every_legacy_command_rejects_json_before_any_effect`, `schema_fixtures` | Verified on Linux |
| FR-008 documented units, nullability, ordering, timestamps, ids, lossless numbers, snapshot limits | encodings tested in `list_contract` and `schema_fixtures`; written down in `docs/commands.md` (Machine output) and the data model | Tested and documented; the prose has no automated check beyond links |
| FR-009 schema versions independent of the CLI version | `schema_version` constant and `executable_version` in the generated fixture, drift tests, `docs/commands.md` (Versioning) | Verified on Linux |
| SC-001 exactly one valid JSON document per controlled scenario | the T031, T040, T046 and T044 process tests | Verified on Linux |
| SC-002 human and machine agree; zero mutations | `order_list_human_and_json_describe_the_same_book` and the no-effects assertions | Verified on Linux |
| SC-003 offline discovery, no identity files | `schema_runs_offline_in_both_modes` | Verified on Linux |
| SC-004 invalid input and failed reads exit nonzero; empty books succeed | the read-failure, configuration and empty-result process tests | Verified on Linux |
| SC-005 native Windows and Linux process checks pass; no secrets | Linux: all process tests and sentinel sweeps pass | Windows: all automated tests passed on a GitHub-hosted native Windows runner (CI run 40934312083, commit `3a4145e`, `cargo test --locked --no-fail-fast`), including the 46 process tests and the secret sentinels; it is a CI runner, not a developer workstation, and the quickstart rows were not walked by hand there | Verified on Linux and on the CI-hosted Windows runner |

No scenario above is marked complete by documentation alone. Remaining blockers for
the whole pilot: live-relay and live-node behavior, a Windows run on a developer
workstation (the CI-hosted run passed, see SC-005), NIP-42 AUTH, and the unmigrated write commands and MCP, which
are outside this pilot.


## Order Info: Requirement Evidence

Reconciliation of the [specification](../specs/002-order-info-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-5), as of 2026-10-29. **Automated verification on Linux with loopback relays
and a scripted fake daemon only.** Not run against any real daemon, regtest or public node; the daemon behavior the fake
encodes is read from the v0.20.1 source (regtest pins v0.19.3). NIP-42 AUTH is never exercised. Slices 4 and 5 have not
yet run on a Windows host. Contracts: [CLI](../specs/002-order-info-json/contracts/cli.md),
[data model](../specs/002-order-info-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json) and [info](../docs/schemas/order-info-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001, FR-001a ids, no cap, order, `not_returned`, `not_found`, removed spellings | `order_info_grammar`, `parser_orders::info_contract`, `a_subset_keeps_request_order_...`, `a_repeated_id_in_any_case_...`, `not_found_is_the_same_success_...`, `many_ids_go_out_in_one_request_...`, `argument_errors_name_the_command_only_when_it_resolved` | Verified on Linux |
| FR-002 read-only, one request, identity key | `one_id_returns_the_order_...`, `a_seeded_identity_is_a_real_store_...`, `identity` unit tests, the matrix's `assert_untouched` | Verified on Linux |
| FR-003 correlation, deadline, cleanup | the `exchange` contract tests in `order_info_process`, `noise_around_the_reply_never_decides_the_result` | Verified on Linux |
| FR-004, FR-006 one envelope, parity, default human | `human_and_json_describe_the_same_orders`, `the_output_flag_works_at_every_position` | Verified on Linux |
| FR-005, FR-007 stages and stable failures | `failures_before_any_request_are_stable_...`, `a_node_that_cannot_be_served_...`, `a_relay_that_rejects_...`, `relay_failures_are_told_apart_...`, the refusal, malformed and timeout tests | Verified on Linux, except: no process-level `internal_error`; a relay that drops the subscription is covered only at the `exchange` level |
| FR-008 discovery, fixtures, unmigrated guard | `schema_fixtures` (drift, examples), `cli_functions` registry tests | Verified on Linux |
| FR-009 shared transport boundary | `source_boundaries` with the updated frozen counts, `compile_fail` doctests | Verified on Linux |
| FR-010 untrusted text, diagnostics | the sentinel sweep in the failure matrix, `verbose_adds_one_allow_listed_line_...`, escape tests | Verified on Linux |
| FR-011 documented contract | `contracts/cli.md`, `docs/commands.md` (the prose has no automated check) | Documented |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted daemon |
| SC-006 Windows and Linux | Linux: all process tests and sentinels | Windows open until CI runs after the push |

No scenario above is marked complete by documentation alone. Open: the regtest probe
([quickstart](../specs/002-order-info-json/quickstart.md)), which needs separate authorization; Windows evidence for
slices 4-5; MCP `get_order`, which breaks until migrated.
