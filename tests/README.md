# Mostro CLI Test Suite

This directory contains comprehensive unit and integration tests for the Mostro CLI application.

## Test Files

Counts are the tests each target lists (`cargo test --locked --test <name> -- --list`,
every `rstest` case counted) on Linux on 2026-11-09, at spec 018 unit 5 and spec 019 unit 4: 791 in
the 24 targets below, plus the library's 314 unit tests and 6 doctests, 1111 in
all. The order-list pilot ended with 428 tests, none ignored. Historical per-file
narratives from earlier refactors are in [TEST_SUMMARY.md](TEST_SUMMARY.md).

1. **`parser_dms.rs`** (19 tests): direct message parsing and display, payload handling, Mostro identification, error cases.
2. **`cli_functions.rs`** (86 tests): command logic and message creation (26); `output_intent` (17: output recovery from malformed argv and the contract decision table, including `schema`) and `registry` (6: the approved canonical names and JSON capability); `maintenance_commands` (4); the grammar of the migrated commands (33, one `*_grammar` module per command group).
3. **`util_misc.rs`** (17 tests): path and string helpers.
4. **`parser_orders.rs`** (123 tests): order event parsing, filters and tables (39); `list_contract` (71: the `order.list` DTO, selection and human-rendering contract); `info_contract` (13: the wire facts of the `order info` fixtures, checked against the pinned `mostro-core`).
5. **`parser_disputes.rs`** (9 tests): dispute event parsing and display.
6. **`integration_tests.rs`** (78 tests): context and filter integration (14); `schema_fixtures` (51: the checked-in `docs/schemas` files against the generated ones, proposed examples, mutations); `source_boundaries` (8: size, imports, transport ownership, no-I/O modules); `ci_wiring` (5: the workflow steps and jobs).
7. **`order_list_process.rs`** (46 tests): runs the real `mostro-cli` binary against in-process loopback relays (OS-assigned ports, generated keys, allow-listed environment, sandboxed `HOME`/`MCLI_DIR`/working directory, closed stdin, 40 s watchdog). Covers listing in both modes, read failures, the event cap, configuration errors, every legacy JSON rejection, offline `schema`, secret sentinels, unwritable streams and no-effects.
8. **`config_file_process.rs`** (10 tests, spec 018): the node settings file `$MCLI_DIR/config` against the real binary, loopback relays and the scripted node: `order list` and `order new` from the file alone, the precedence option > set variable > file counted by relay connections, a set `RELAYS` that is not UTF-8 beating the file (Unix), a refused file at five entry points in both modes (one `hint:`, nothing echoed, contacted or written), the local commands that never read it, a missing data directory that stays missing, the human-only `listdisputes` with the same precedence and stopped by a refused file before `mcli.db` exists, or by a set but empty `MOSTRO_PUBKEY` or `RELAYS` that hides the file (the missing-setting message, whose hint says so), and the human-only `admsenddm` mining at least the file's `POW` with its `TRANSPORT` (no probe).
9. **`config_process.rs`** (17 tests, spec 018): `config set` and `config show` against the real binary. `config set`: a new directory and file (owner-only on Unix), the directory holding only `config` and `.config.lock`, one value replaced in a hand-written file that is rewritten as `NAME=value` lines, refused values and an unknown key (exit 2, nothing echoed or written), a refused file and an unreachable directory left untouched (one `hint:`), unusable data directories (`configuration_unwritable`), human output, `--verbose`, a closed stdout, and no network, stdin or `--yes`. `config show`: the path and the stored values (null when absent) in both modes, no file and a missing data directory left missing, only the file reported (not the options or environment, nothing contacted), a refused file and an unreachable directory (Unix) as `configuration_invalid` with one `hint:` and nothing echoed, `--verbose` and a closed stdout. Both: what `config set` writes is what `config show` reports and `order list` reads, and the `hint:` escapes control characters in the path (Unix).
10. **`order_info_process.rs`** (39 tests, spec 002): `order info` against the scripted node: the fake daemon's self-tests, one request for many ids, refusals, silence and relay failures told apart, noise around the reply, the advertised work, unwritable streams and no effects.
11. **`order_new_process.rs`** (49 tests, spec 003): `order new` against the scripted node: the legacy wire characterization and fixture self-tests (10), the trade-index store (`store`, 10) and the command (`operation`, 29): acceptance, bonds, ranges, refusals, silence, stale indexes, saves that fail and concurrent runs.
12. **`order_take_process.rs`** (35 tests, spec 004): `order take-sell` and `order take-buy` against the scripted node: scripted-daemon self-tests (6) and the commands (`operation`, 29): hold and bond invoices, range amounts, refusals, silence, saves that fail and the prompt.
13. **`invoice_process.rs`** (49 tests, spec 005): `order add-invoice` and `bond add-invoice` against the scripted node: scripted-daemon self-tests (9), the store (`store`, 6) and the commands (`operation`, 34): acceptance, refusals with their reason, decoys, silence, invalid invoices never echoed.
14. **`message_list_process.rs`** (49 tests, spec 006): `message list`: the mailbox fixture (`mailbox`, 5), the legacy `getdm` characterization (`legacy_getdm`, 7), the store (`store`, 9) and the command (`operation`, 28): windows, the key and record caps, duplicates, relay failures, node text never reaching the terminal raw.
15. **`step_process.rs`** (62 tests, spec 007, spec 008): `order fiat-sent`, `order release` and `order cancel` against the scripted node: scripted-daemon self-tests (11), range children (`children`, 5), the store (`store`, 5) and the commands (`operation`, 41).
16. **`sync_process.rs`** (19 tests, spec 008): `order sync`: announced orders adopted with the key of their index, known orders never replaced, failed saves, and the real binary (`binary`, 10) in every mode, with no secret on any stream.
17. **`identity_process.rs`** (14 tests, spec 009): `identity init` and `identity show` with no node: the store the legacy first run creates, the key shown and never the mnemonic, a second or concurrent init, damaged or missing stores, an unwritable stdout.
18. **`invoice_stdin_process.rs`** (7 tests, spec 010): `--invoice -` on the four commands that take an invoice: the line sent, `--yes` required, invalid input refused before any effect, the `invalid_invoice` reason and its schema.
19. **`order_mine_process.rs`** (6 tests, spec 011): `order mine` over the local store, newest first, with no node: an empty, missing or damaged store and rows whose index is not found.
20. **`expires_in_process.rs`** (4 tests, spec 012): `order new --expires-in` against the scripted node: the expiry sent, the limits, the human preview and invalid or conflicting values.
21. **`dispute_rate_process.rs`** (14 tests, spec 013, spec 014): `dispute open` (`dispute`, 6) and `order rate` (`rate`, 6) against the scripted node, the removed legacy commands, and answers that name another order.
22. **`bond_flows_process.rs`** (4 tests, spec 015): `order cancel` of a take or an order that waits on its bond, and `message list` over the bond messages.
23. **`reply_timeout_process.rs`** (4 tests, spec 016): `--reply-timeout`: its range, every waiting command stopped at the deadline, a late answer taken only with a longer one, public reads keeping their own limits.
24. **`order_chat_process.rs`** (31 tests, spec 019): `order chat` and its building blocks against isolated stores and loopback relays: the `counterparty_pubkey` column read without writing and written alone (`store`), a stored key used with no relay contacted, the key found in post-lock mail and saved, `counterparty_unknown` before the lock, store and relay failures (`counterparty`), and the chat transport: one bounded read of both sides, a truncated read, a sent message the counterparty opens, confirmations, refusals and unreachable relays (`transport`, 7), and the command itself (`operation`, 5; `binary`, 6; `json`, 3).
25. **`common/schema_check.rs`**: the small JSON Schema checker shared with the `outcome::schema` unit tests (not a test target).

The library also carries 314 unit tests and 6 doctests (the `msg` privacy examples, five of them `compile_fail`).

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

No scenario above is marked complete by documentation alone. The regtest probe ran on 2026-11-01; still open: bonded and Cashu nodes and the cases
listed in TEST_SUMMARY.md
([quickstart](../specs/002-order-info-json/quickstart.md)); MCP `get_order`, which breaks until migrated.

## Order New: Requirement Evidence

Reconciliation of the [specification](../specs/003-order-new-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-6), as of 2026-10-30. **Automated verification on Linux with loopback relays and a
scripted fake daemon only**, plus the CI-hosted Windows runner for slices 1-5. Not run against any real daemon, regtest or public
node; the daemon behavior the fake encodes is read from the v0.20.1 source (regtest pins v0.19.3). NIP-42 AUTH is never exercised.
Contracts: [CLI](../specs/003-order-new-json/contracts/cli.md), [data model](../specs/003-order-new-json/data-model.md); generated
references: [discovery](../docs/schemas/discovery-v1.json) and [order new](../docs/schemas/order-new-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 options, validation, removed spellings | `order_new_grammar`, `outcome::new` unit tests, `invalid_input_is_an_argument_error_...` | Verified on Linux |
| FR-002 existing identity only, nothing created | the identity cases of `failures_before_any_request_...` | Verified on Linux |
| FR-003 `--yes` in JSON, prompt in human, declined is a stable outcome | `json_without_yes_...`, `the_prompt_accepts_y_and_blank_...` | Verified on Linux |
| FR-004 one request, trade key, index reserved late | `an_accepted_order_is_returned_and_saved_...`, the exchange and store tests, refusal and local-failure cases that leave the index unspent; amended 2026-11-05 by spec 008 (recorded before publishing): `a_run_killed_after_publishing_...`, `two_runs_at_once_...`, the reservation store tests; a refusal for a stale index keeps it (review of slice 7): `a_stale_index_refusal_...`, `an_index_the_node_recorded_before_refusing_...` | Verified on Linux |
| FR-005 correlation, deadline, cleanup | the `exchange` tests incl. a reply addressed to the identity never heard | Verified on Linux |
| FR-006 save with the index, `order_not_saved` | the store transaction tests, `an_order_that_cannot_be_saved_...` (twice, two failure shapes) | Verified on Linux |
| FR-007, FR-008 statuses, stages, stable codes, timeout is `uncertain` | `outcome::new` schema tests, the binary scenarios, `failures_before_any_request_...` | Verified on Linux, except: no process-level `internal_error`; a relay that drops the wait is covered only at the `exchange` level |
| FR-009 one document, human parity, no secret | `json_with_yes_...` (trade key and mnemonic read from the store and searched for), the human tests | Verified on Linux |
| FR-010 schemas, fixtures, drift | `integration_tests` (fixture, examples, invalid variants) | Verified on Linux |
| FR-011 shared transport boundary, untrusted text | `source_boundaries`, escape and sentinel tests | Verified on Linux |
| FR-012 `neworder` removed | `the_legacy_spellings_are_gone`, the removed-spelling cases | Verified on Linux |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted daemon |
| SC-006 no claim about a daemon | regtest probe on mostrod v0.19.3 (2026-11-01, see TEST_SUMMARY.md) | Verified on regtest for the main paths and refusals; bonded and Cashu nodes, take-window and price cases are not verified |

No scenario above is marked complete by documentation alone. The regtest probe ran on 2026-11-01; still open: bonded and Cashu nodes and the cases
listed in TEST_SUMMARY.md
([quickstart](../specs/003-order-new-json/quickstart.md)), the MCP `create_order` migration (it stays broken until then), and the
duplicate-order risk of an `uncertain` run, which only the caller can check with `order list`.

## Order Take: Requirement Evidence

Reconciliation of the [specification](../specs/004-order-take-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-6), as of 2026-11-01. **Automated verification on Linux with loopback relays and a
scripted fake daemon only**, plus the CI-hosted Windows runner for slices 1-5 (slice 6 is documentation). Not run against any real daemon, regtest or
public node; the daemon behavior the fake encodes is read from the v0.20.1 source (regtest pins v0.19.3). NIP-42 AUTH is never exercised. Contracts:
[CLI](../specs/004-order-take-json/contracts/cli.md), [data model](../specs/004-order-take-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json), [take-sell](../docs/schemas/order-take-sell-v1.json) and [take-buy](../docs/schemas/order-take-buy-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 options, validation, removed spellings | `order_take_grammar`, `outcome::take` unit tests, `invalid_input_is_an_argument_error_...` | Verified on Linux |
| FR-002 existing identity only, nothing created | the identity cases of `failures_before_any_request_...` (both takes) | Verified on Linux |
| FR-003 `--yes` in JSON, prompt in human, declined is a stable outcome | `json_without_yes_does_nothing_...`, `the_prompt_accepts_y_and_blank_...` | Verified on Linux |
| FR-004 one request, trade key, index reserved late, no extra read | the operation tests, the refusal and local-failure cases that leave the index unspent (given back since 2026-11-05, spec 008), a stale-index refusal that keeps it (`refusals_carry_their_reason_...`, `a_stale_index_refusal_...`), one request per run | Verified on Linux |
| FR-005 correlation, deadline, cleanup | the `exchange` tests of `order new` and the replies that do not fit the take | Verified on Linux |
| FR-006 next step as data, hold invoice only in the result | the three success tests, `a_take_buy_returns_the_hold_invoice_...`, the stream sweeps | Verified on Linux |
| FR-007 row and index saved together, minimal row, unsaved is `uncertain` | the operation tests, both unsaved-take tests | Verified on Linux |
| FR-008, FR-009 statuses, stages, stable codes | `outcome::take` schema tests, the binary scenarios, the failure matrix | Verified on Linux, except: no process-level `internal_error`; Cashu escrow replies are only covered as an unusable reply |
| FR-010, FR-011 one document, own schemas, fixtures, drift | `integration_tests` (fixtures, examples, invalid variants) | Verified on Linux |
| FR-012 transport boundary, untrusted text, diagnostics | `source_boundaries`, the sentinel and hold-invoice sweeps | Verified on Linux |
| FR-013 legacy takes removed | `the_legacy_spellings_are_gone`, the removed-spelling cases | Verified on Linux |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted daemon |
| SC-006 no claim about a daemon | regtest probe on mostrod v0.19.3 (2026-11-01, see TEST_SUMMARY.md) | Verified on regtest for the main paths and refusals; bonded and Cashu nodes, take-window and price cases are not verified |

No scenario above is marked complete by documentation alone. The regtest probe ran on 2026-11-01; still open: bonded and Cashu nodes and the cases
listed in TEST_SUMMARY.md
([quickstart](../specs/004-order-take-json/quickstart.md)), the MCP `take_order` migration (it stays broken until then), the minimal row's empty fields
until a later command refreshes them, and the duplicate-take risk of an `uncertain` run, which only the caller can check with `order info --order <id>`.

## Add Invoice: Requirement Evidence

Reconciliation of the [specification](../specs/005-invoice-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-6), as of 2026-11-01. **Automated verification on Linux with loopback relays and a
scripted fake daemon only**; CI (with the CI-hosted Windows runner) was green through slice 4b when this was written. `order add-invoice` was also probed on the maintainer's
local regtest node on 2026-11-01 (accepted and three refusals, see SC-006 below); `bond add-invoice` was not run against any real node;
the daemon behavior the fake encodes is read from the v0.20.1 source (regtest pins v0.19.3). NIP-42 AUTH is never exercised. Contracts:
[CLI](../specs/005-invoice-json/contracts/cli.md), [data model](../specs/005-invoice-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json), [order add-invoice](../docs/schemas/order-add-invoice-v1.json) and
[bond add-invoice](../docs/schemas/bond-add-invoice-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 options, validation, removed spellings | `invoice_grammar`, `outcome::invoice` unit tests, `invalid_input_is_an_argument_error_...` | Verified on Linux |
| FR-002 existing identity only, key from the store, `order_unknown`, nothing created | the identity cases of `failures_before_any_request_...`, the `store` tests, `an_unknown_order_...` | Verified on Linux |
| FR-003 `--yes` in JSON, prompt in human, preview never shows the invoice | `json_without_yes_does_nothing_...`, `the_prompt_accepts_y_and_blank_...` | Verified on Linux |
| FR-004 one request, no trade index, no retry | the operation tests (one request, no index, index unchanged), the refusal and failure cases | Verified on Linux |
| FR-005 correlation including the order-id match, deadline, cleanup | `util::reply` unit tests, the decoy tests for both commands, the ack with no request id | Verified on Linux |
| FR-006 statuses and stages per case | `outcome::invoice` schema tests, the operation tests, the binary scenarios | Verified on Linux |
| FR-007 local status follows the reply; a failed update is a warning | `an_invoice_for_a_waiting_order_...`, `a_failed_status_update_...`, `a_stale_local_status_...` | Verified on Linux; the status update on `hold-invoice-payment-accepted` is new behavior, not a migration |
| FR-008 one document, own schemas, fixtures, examples, drift | `integration_tests` (fixtures, 15 examples, invalid variants) | Verified on Linux |
| FR-009 transport boundary, untrusted text, diagnostics | `source_boundaries`, the sentinel and invoice sweeps | Verified on Linux |
| FR-010 legacy commands removed | `the_legacy_commands_are_gone_with_no_alias`, the removed-spelling cases | Verified on Linux |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted daemon |
| SC-006 no claim about a daemon | probe of 2026-11-01 on the maintainer's local regtest node (see TEST_SUMMARY.md) | Verified on a real node for `order add-invoice` (accepted, three refusals); **open** for `payment_accepted`, `invoice_updated`, the node-stopped case and all of `bond add-invoice` |

No scenario above is marked complete by documentation alone. Still open: the rest of the `order add-invoice` probe
([quickstart](../specs/005-invoice-json/quickstart.md)), a slashed bond for `bond add-invoice` (including that the acknowledgement carries no request id), the
the MCP (which has no invoice tool), and the duplicate-send risk of an `uncertain` run, which only the caller can check with
`order info --order <id>` (for a bond payout, by waiting for the payout).

## Message List: Requirement Evidence

Reconciliation of the [specification](../specs/006-message-list-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-6), as of 2026-11-02. **Automated verification on Linux with loopback relays and a scripted
mailbox only**; CI (with the CI-hosted Windows runner) is checked after the push. Probed once on a real node (below); whether real relays honor one
filter with many recipient keys beyond two, and what limit they apply, is unprobed. NIP-42 AUTH is never exercised. Contracts:
[CLI](../specs/006-message-list-json/contracts/cli.md), [data model](../specs/006-message-list-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json) and [message list](../docs/schemas/message-list-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 options, validation, removed spellings | `message_list_grammar`, `outcome::messages` unit tests, `bad_arguments_exit_2_...`, `the_old_command_is_gone_with_no_alias` | Verified on Linux |
| FR-002 existing identity only, nothing created or written | the identity cases of `failures_before_any_relay_...`, `a_read_only_store_is_read_without_complaint`, the byte-identical store checks | Verified on Linux |
| FR-003 keys 1..last (newest 200), `--order`, `order_unknown` | the `store` tests, `250 trades` and `--order` operation tests, `an_unknown_order_and_a_missing_identity_...` | Verified on Linux |
| FR-004 one bounded read, `order list` completion rules, snapshot | the operation and relay-failure tests, `an_unreachable_relay_...` | Verified on Linux |
| FR-005 author, key, window, shape, duplicates, caps, oldest first | `what_is_not_the_nodes_mail_...`, the duplicate, relay-limit and record-cap tests, `open_events` unit tests | Verified on Linux |
| FR-006 record fields, `next_step`, payload summary, escaped text, invoices as data | `outcome::messages` tests, the renderer tests, `text_from_the_node_cannot_reach_the_terminal_raw`, `human_shows_each_invoice_alone_...` | Verified on Linux |
| FR-007 one document, own schema, fixtures, examples, drift | `integration_tests` (fixture, 10 examples, 12 invalid variants) | Verified on Linux |
| FR-008 statuses, exits, codes, empty window is `ok` | the failure matrix, `no_messages_in_the_window`, the binary scenarios | Verified on Linux |
| FR-009 transport boundary, diagnostics | `source_boundaries`, the sentinel, relay-address and invoice sweeps | Verified on Linux |
| FR-010 `getdm` removed | `message_list_grammar`, the registry tests, `the_old_command_is_gone_with_no_alias` | Verified on Linux; `get_dm.rs` stays for `getadmindm` |
| FR-011 work and transport settings ignored by a read | `work_and_transport_settings_do_not_concern_a_read` | Verified on Linux |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted mailbox |
| SC-006 no claim about a real node | none | Probe of 2026-11-02 on the maintainer's local bonded regtest node (see TEST_SUMMARY.md): **verified on a real node** for the bond/add-invoice flow, two keys in one filter and `canceled`; **open**: many keys (up to 200), a relay's per-relay limit, clock skew ([quickstart](../specs/006-message-list-json/quickstart.md)) |

No scenario above is marked complete by documentation alone. Still open: the probe, the MCP DM tools (broken until migrated), a cursor for slow flows, peer chat as
a command of its own, and a smoother solver-key flow for disputes (`getdm` used to save the solver's key; `message list` returns it as data).

## Trade Steps: Requirement Evidence

Reconciliation of the [specification](../specs/007-trade-steps-json/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-6), as of 2026-11-02. **Automated verification on Linux with loopback relays and a scripted node
only**; CI (with the CI-hosted Windows runner) is checked after the push. Probed once on a real node (below); the daemon behavior the fake encodes is read from the v0.20.1 source. NIP-42 AUTH is never exercised. Contracts:
[CLI](../specs/007-trade-steps-json/contracts/cli.md), [data model](../specs/007-trade-steps-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json), [fiat-sent](../docs/schemas/order-fiat-sent-v1.json),
[release](../docs/schemas/order-release-v1.json) and [cancel](../docs/schemas/order-cancel-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 options, removed spellings | `trade_step_grammar`, `invalid_input_is_an_argument_error_...` | Verified on Linux |
| FR-002 existing identity only, key from the store, `order_unknown`, nothing created | the identity cases of `failures_before_any_request_...`, the `store` tests, `an_unknown_order_...` | Verified on Linux |
| FR-003 `--yes` in JSON, prompt in human | `json_without_yes_does_nothing_...`, `the_prompt_accepts_y_and_blank_...` | Verified on Linux |
| FR-004 one request, no retry, no index | the operation tests (one publication, index unchanged), the refusal and failure cases | Verified on Linux |
| FR-005 correlation by request id, deadline, cleanup | `util::reply` unit tests, the decoy tests, `a_reply_without_the_request_id_...` | Verified on Linux; the spec's "order id fallback" was dropped because the node echoes the request id |
| FR-006 statuses and stages per case | `outcome::step` schema tests, the operation tests, the binary scenarios | Verified on Linux |
| FR-007 data, next step, the range key | `outcome::step` unit tests, the range tests of spec 008 | Verified on Linux; the warning `range_not_continued` was replaced by the key and index of spec 008 |
| FR-008 one document, own schemas, fixtures, examples, drift | `integration_tests` (fixtures, 16 examples, 17 invalid variants) | Verified on Linux |
| FR-009 transport boundary, untrusted text, diagnostics | `source_boundaries`, the sentinel sweeps | Verified on Linux |
| FR-010 legacy arms removed | the removed-spelling cases, `cli/send_msg.rs` serving only `dispute` and the admin commands (removed with `dispute` by spec 013, 2026-11-05) | Verified on Linux |
| SC-001 to SC-005 | the process tests above | Verified on Linux against the scripted node |
| SC-006 no claim about a daemon | probe of 2026-11-02 on the maintainer's local regtest node (see TEST_SUMMARY.md) | Verified on a real node for a whole trade, the refusals, plain and cooperative cancel; **open** for a range order, `uncertain` and a disputed order |

No scenario above is marked complete by documentation alone. Still open: the rest of the regtest probe (a range order, `uncertain`), spec 008 (range children:
continuing a range order, which these commands do not do), the MCP (which has no tool for these commands),
and the duplicate-send risk of an `uncertain` run, which only the caller can check with `order info --order <id>`.

## Range Children: Requirement Evidence

Reconciliation of the [specification](../specs/008-range-children/spec.md) with the entries in
[TEST_SUMMARY.md](TEST_SUMMARY.md) (slices 1-7), as of 2026-11-05. **Automated verification on Linux with loopback relays, a scripted node and a
scripted mailbox only**; CI (with the CI-hosted Windows runner) is checked after the push. Not run against any real daemon, regtest or public node; the
daemon behavior the fake encodes is read from the v0.20.1 source (regtest pins v0.19.3). NIP-42 AUTH is never exercised. Contracts:
[CLI](../specs/008-range-children/contracts/cli.md), [data model](../specs/008-range-children/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json), [order sync](../docs/schemas/order-sync-v1.json) and the amended
[fiat-sent](../docs/schemas/order-fiat-sent-v1.json) and [release](../docs/schemas/order-release-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 exact maker marker, `range_has_room` needs the user's own | `the_maker_marker_is_exact_...`, `no_range_warning_for_the_taker_...` (before the warning was removed), the marker assertions in the `order new` and `order take-sell` process tests | Verified on Linux |
| FR-002 key at the maker's own step, index recorded before publishing and given back only after a refusal or a request that never left (slice 7) | `the_makers_own_step_...`, `no_key_for_...`, `the_index_is_spent_...`, `an_index_that_cannot_be_reserved_...`, `sends_key` unit tests | Verified on Linux |
| FR-003 `next_trade_index`, `sync_child_order` | `outcome::step` unit tests, the binary tests, the examples | Verified on Linux |
| FR-004 `order sync` adds unknown announced orders, idempotent, never replaces, publishes nothing | `tests/sync_process.rs` operation and binary tests | Verified on Linux; the confirmation rule was dropped by amendment |
| FR-005 data fields, no secrets | the schema and example tests, `no_secret_reaches_a_stream_...` | Verified on Linux |
| FR-006 statuses, codes, one document, schema, fixtures, examples, drift | `integration_tests` (fixture, 10 examples, 15 invalid variants), the failure matrix | Verified on Linux |
| FR-007 spec 007 documents amended | the amended spec, data model, contract, command reference and quickstart of spec 007 | Verified by reading |
| SC-001 a range child is created and usable | `mod children` (the scripted node), `an_adopted_child_can_be_acted_on_at_once` | Verified on Linux against the scripted node |
| SC-002 no key for a taker, a fixed order, an exhausted range, a cancel | `no_key_for_...` | Verified on Linux |
| SC-003 `order sync` writes only new rows, idempotent, byte-identical on failure | `a_second_run_...`, `a_known_order_...`, the failure matrix | Verified on Linux |
| SC-004 human and JSON agree, no secret, CI | the binary tests; CI after the push | Verified on Linux |
| SC-005 no claim about a daemon | probe of 2026-11-03 on the maintainer's local regtest node (see TEST_SUMMARY.md) | Verified on a real node for a sell range order (key, child announcement, `order sync`, cancel); **open** for a buy range order and an exhausted slice |
| SC-006 a killed run never reuses its key, concurrent runs never share an index, every writer only raises it (slice 7) | `a_continuation_killed_after_publishing_...`, `a_run_killed_after_publishing_...` (SIGKILL/TerminateProcess of the real binary after the node received the request), `two_continuations_at_once_...`, `two_runs_at_once_...`, `concurrent_reservations_...` (8), `a_reservation_that_cannot_commit_...`, `a_give_back_restores_...`, `last_trade_index_never_lowers_...`, the raise-only legacy save | Verified on Linux against the scripted node; not on a real node; the Windows runner is checked after the push |

No scenario above is marked complete by documentation alone. Still open: the regtest probe of a buy range order and of an exhausted slice, and the accepted limits: the
maker's local `fiat_amount` is the creation value, so the key may be sent when the taken slice leaves no room (one index is spent); rows saved before
the maker marker was exact keep their old marker; the MCP has no tool for these commands.


## Identity Commands: Requirement Evidence

Reconciliation of the [specification](../specs/009-identity-json/spec.md) with the entry in [TEST_SUMMARY.md](TEST_SUMMARY.md), as of
2026-11-05. **Automated verification on Linux with isolated stores only.** The commands talk to no node or relay, so there is nothing to probe on
regtest; the CI Windows runner is checked after the push (the 0600/0700 modes are Unix-only, as for the legacy path). Contracts:
[CLI](../specs/009-identity-json/contracts/cli.md), [data model](../specs/009-identity-json/data-model.md); generated references:
[discovery](../docs/schemas/discovery-v1.json), [identity init](../docs/schemas/identity-init-v1.json) and
[identity show](../docs/schemas/identity-show-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 grammar, names, registry, discovery, root options ignored | `identity_grammar`, `registry`, `the_identity_commands_need_no_node_configuration`, `options_are_argument_errors_before_any_effect`, `root_options_are_ignored_and_no_node_configuration_is_needed` | Verified on Linux |
| FR-002 the legacy store, no node, no secret | `init_creates_the_store_the_legacy_first_run_creates` (compared with a store made by the legacy first run), `init_in_json_reports_the_public_key_only_and_needs_no_node`, `store::create` unit tests | Verified on Linux |
| FR-003 existing store or companion, unwritable directory, half-created file, two runs at once | `a_second_init_reports_identity_exists_and_changes_nothing`, `init_refuses_a_leftover_companion_file`, `a_directory_that_cannot_be_created_is_store_unwritable`, `two_inits_at_once_create_one_store`, `discarding_removes_the_store_and_its_companions_only` | Verified on Linux, except: a write that fails after the file was created (`failed`) has no process-level case, only the unit test of the removal |
| FR-004 show, read-only, missing and damaged stores | `show_reports_the_key_and_the_last_trade_index_and_changes_nothing`, `show_without_a_store_...`, `show_on_a_damaged_store_...`, the `identity` unit tests (index range, WAL without its `-shm` file) | Verified on Linux |
| FR-005 no mnemonic or private key on any stream, human and JSON agree | the secret sweeps of the process tests, `outcome::identity` and `parse_human::identity` unit tests | Verified on Linux |
| FR-006 one document, schemas, fixtures, examples, drift | `identity_schema_fixtures_match_the_generated_schemas`, `proposed_identity_examples_...`, `identity_examples_are_reproduced_...`, `invalid_identity_variants_do_not_conform` | Verified on Linux |
| FR-007 legacy first run unchanged | the db unit tests of `create_private_db_file`, the legacy run of the comparison test | Verified on Linux |
| SC-001 to SC-005 | the process tests above, `a_store_from_identity_init_is_used_at_once_by_order_new` (scripted node) | Verified on Linux |
| SC-006 Windows | none yet | Open until CI runs after the push |

## Invoice From Stdin: Requirement Evidence

Reconciliation of the [specification](../specs/010-invoice-stdin/spec.md) with the entry in [TEST_SUMMARY.md](TEST_SUMMARY.md), as of 2026-11-05.
**Automated verification on Linux against the scripted node only.** Contracts: [CLI](../specs/010-invoice-stdin/contracts/cli.md),
[data model](../specs/010-invoice-stdin/data-model.md); generated reference: [discovery](../docs/schemas/discovery-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| FR-001 `-` reads stdin on the four commands, a literal value is unchanged | `the_invoice_on_stdin_is_the_one_sent_by_every_command`, `a_literal_invoice_is_unchanged`, `any_other_value_is_kept_as_it_is_...` | Verified on Linux |
| FR-002 bounded, strict, `invalid_arguments` before any effect | `invalid_input_on_stdin_is_an_argument_error_before_any_effect` (empty, blank, two lines, more than 8 KiB, invalid UTF-8; both modes; nothing sent; store byte-identical), the `cli::stdin` unit tests (the 8192-byte limit, at most 8193 bytes read; a terminal as stdin refused at once without a read, `a_terminal_as_stdin_fails_at_once_without_being_read`) | Verified on Linux; the terminal case only with an injected flag (no pseudo-terminal in the harness) |
| FR-003 `--yes` required, stdin untouched without it | `without_yes_stdin_is_never_read` (stdin held open), `the_dash_needs_yes_and_without_it_stdin_is_never_touched` | Verified on Linux |
| FR-004 validation as for a literal value | `the_invoice_on_stdin_is_the_one_sent_by_every_command` (BOLT-11 and the bond payout invoice), `human_mode_with_yes_reads_one_trimmed_line` | Verified on Linux |
| FR-005 never echoed | the sentinel sweeps of the process tests | Verified on Linux |
| FR-006 discovery marker, regenerated discovery | `the_invoice_option_is_marked_as_readable_from_stdin`, `discovery_fixture_matches_the_generated_document` | Verified on Linux |
| FR-007 `invalid_invoice` on a refused invoice value only (amended 2026-11-05 after review) | `an_invoice_the_client_refuses_carries_the_reason_invalid_invoice` (literal and stdin, JSON and human; `-` without `--yes` and other argument errors keep no reason), `the_schemas_tie_the_reason_to_its_code`, the output fixture drift tests and the 003-005 examples | Verified on Linux |
| SC-001 to SC-005 | the tests above | Verified on Linux against the scripted node; the CI Windows runner is checked after the push |

## Order Mine, Expiry In Seconds, Dispute Open, Order Rate, Bond Flows: Requirement Evidence

Reconciliation of specs [011](../specs/011-order-mine/spec.md), [012](../specs/012-order-expires-in/spec.md),
[013](../specs/013-dispute-open/spec.md), [014](../specs/014-order-rate/spec.md) and [015](../specs/015-bond-flows/spec.md) with the entry in
[TEST_SUMMARY.md](TEST_SUMMARY.md), as of 2026-11-05. **Automated verification on Linux with isolated stores, the scripted node and the
scripted mailbox only**; no real node; the CI Windows runner is checked after the push. Generated references:
[discovery](../docs/schemas/discovery-v1.json), [order mine](../docs/schemas/order-mine-v1.json),
[dispute open](../docs/schemas/dispute-open-v1.json), [order rate](../docs/schemas/order-rate-v1.json).

| Requirement | Evidence | Status |
| --- | --- | --- |
| 011 FR-001 to FR-004 grammar, data, read-only, errors | `tests/order_mine_process.rs` (6: the rows `order new`, `order take-sell` and an adoption saved, newest first, with index and key; empty; no store; damaged store and unknown status; key without index and `trade_keys_limited`; options), `outcome::mine` unit test | Verified on Linux |
| 011 FR-005 schema, examples, drift | `mine_rate_and_dispute_schema_fixtures_match_...`, `mine_dispute_and_rate_examples_conform_and_are_reproduced`, the invalid variants | Verified on Linux |
| 012 FR-001 to FR-003 | `tests/expires_in_process.rs` (4: `now + 60` sent, the preview line, out of range and conflict refused before any effect, both limits accepted), `outcome::new` unit test, `order_new_lists_expires_in` | Verified on Linux against the scripted node |
| 013 FR-001 to FR-004 | `tests/dispute_rate_process.rs` `mod dispute` (5: opened with the dispute id and local status `dispute`; preview, decline and refusal; a second dispute `uncertain`; no `--yes` and an unknown order send nothing; legacy spellings gone), `outcome::dispute` unit test | Verified on Linux against the scripted node |
| 014 FR-001 to FR-004 | `mod rate` (5: rated with nothing written; prompt and refusal; a second rating `uncertain`; ratings 0, 6, 255, text and a missing one refused; legacy spellings gone), `outcome::rate` unit tests | Verified on Linux against the scripted node |
| 013, 014 FR-005 | the schema fixtures, examples and invalid variants above | Verified on Linux |
| 015 FR-001 | `tests/bond_flows_process.rs` (a taker cancels a bond-parked take, a maker cancels a bond-parked order, a stranger is refused) | Verified on Linux against the scripted node; passed on the unchanged code |
| 015 FR-002 | `message_list_reports_the_bond_messages` | Verified on Linux against the scripted mailbox; passed on the unchanged code |

No scenario above is marked complete by documentation alone. Open: a real node for all five (in particular the silence that answers a second
dispute or rating, and a maker bond that locks during a cancel), the MCP.

Review follow-up (2026-11-05): `an_order_saved_between_the_reads_never_lacks_its_index` (011, rows read before keys),
`a_refusal_that_names_another_order_or_none_is_uncertain` (013, 014), `a_terminal_as_stdin_fails_at_once_without_being_read` (010); see the
review entry in [TEST_SUMMARY.md](TEST_SUMMARY.md). The invoice reason (2026-11-05): `an_invoice_the_client_refuses_carries_the_reason_invalid_invoice`,
`the_schemas_tie_the_reason_to_its_code` (`tests/invoice_stdin_process.rs`) and the `invalid_invoice` examples of specs 003-005.

Reply deadline (spec 016, 2026-11-05): `tests/reply_timeout_process.rs` (4) and `the_global_options_are_listed_with_what_they_apply_to`;
verified on Linux against the scripted node only. The step refusal rule (follow-up of spec 013):
`a_refusal_that_names_another_order_or_none_is_uncertain_for_every_step`.

Order chat (spec 019, unit 1 of 4, 2026-11-08): `tests/order_chat_process.rs` (10: `mod store`, the counterparty column read without
writing and written alone; `mod counterparty`, a stored key used with no relay contacted, the lookup from eight-day-old lock mail and from
`fiat-sent-ok`, `counterparty_unknown` before the lock, our own key in the row, a refused save, store failures before any relay, relay
failures) and the
`msg::counterparty` unit tests (8, the impostor case included); verified on Linux against isolated stores and loopback relays only. No
command uses the lookup yet. Unit 2 (2026-11-08): `tests/order_chat_process.rs` `mod transport` (7: one bounded read of both sides,
a relay at its limit of 200 marking the read truncated, a sent message the counterparty opens and that is one kind-14 event authored by
`K_sign` with one `p` tag, one confirmation beside a refusing relay while a relay asking for NIP-42 auth still gets the message, refused,
unconfirmed and unreachable relays) and the unit tests of `msg::chat` (3: replays, forged envelopes and third writers; the window and the
cap of 200; the 60-second clock bound), `outcome::chat` (4, the D4 stage combinations included), `parse_human::chat` (4) and
`parse_human::table::escape_untrusted` (1, the joiners kept); loopback relays only, no command yet. Unit 3 (2026-11-09):
`tests/order_chat_process.rs` `mod operation` (5: a read of both sides that learns and caches the counterparty, `order_unknown` and
`counterparty_unknown` before any chat read even with a message and the store codes, a send kept apart from the conversation and read back
once, refused and unconfirmed sends, a failed read that sends nothing) and `mod binary` (6: the human conversation with forged lines and
bidirectional controls escaped, a send without a question and `--yes` alone sending nothing, bad `--since` and `--message` (a 4097-byte one
included) exiting 2 before any relay and `--message=-x` accepted, an uncertain send printing the advice line and still the conversation, a
closed stdout giving one fixed line, no secret on any stream and one `--verbose` line); `chat_examples_conform_and_are_reproduced` and
`invalid_chat_variants_do_not_conform` (`tests/integration_tests.rs`); `order.chat` in the registry mapping and in
`every_legacy_command_rejects_json_before_any_effect`. Unit 4 (2026-11-09): `mod json` (3: JSON `--message` without `--yes` refused with
no relay, store or configuration touched, a read and a send conforming to `docs/schemas/order-chat-v1.json`, `counterparty_unknown` with
`--message --yes`), `chat_schema_fixture_matches_the_generated_schema`, and `order.chat` among the machine commands (31 tests in the file).

Order watch (spec 020, unit 1 of 7, 2026-11-09): the unit tests of `outcome::watch` (3: the `--since`, `--timeout` and `--after` bounds
and defaults; the ending actions; `WatchData` outcomes checked against the generated schema), `outcome::watch::cursor` (5: round trips
and 22 malformed cursors refused; the next cursor's time and ids; the cap of 200 raising the time only over records older than a minute;
records stamped ahead never moving the time past now minus 60 s, and node messages kept ahead of chat dated ahead when the cap binds; a
chat message held over five simulated calls) and the four `TradeRole` pairs in `outcome::messages`. No relay, store or command.

Order watch (spec 020, unit 2 of 7, 2026-11-09): `tests/order_watch_process.rs` `mod live` (5: the window replayed once from two relays,
one answering every REQ with all it holds, then new mail live once, foreign or old events never counted; the chat replayed and live when
the counterparty is known, a re-wrapped line once, a line dated before the window in an envelope inside it kept, a third writer's dropped;
50 forged envelopes hiding an older line set `truncated`; the opening failures; a bounded close) and `msg::watch::state` (8).

Order watch unit 3 (2026-11-09): `tests/order_watch_process.rs` `mod reconnect` (5, with a 1 s retry: a relay dropped after the
opening replay reconnects and delivers once the mail stored while it was away, without repeating the window; `follow_chat` while one
relay is away returns without waiting for it, the other relay's chat comes first and the away relay's line after its reconnect; a chat
REQ the relay refused with CLOSED is not asked again while connected and is sent again on reconnect, and after a `rate-limited:` CLOSED
the relay gets exactly one chat REQ on reconnect (four REQs in all); 51 mail events stored during an
outage come back as 50 with `truncated`) and `msg::watch::state` (11 with the reconnect replay, a live replay of 50 cut before EOSE or by a CLOSED, and the dropped feeds). The harness
`Backend::Kickable(relay, notify)` serves like `Backend::Relay`; `LoopbackRelay::kick` ends every open session and keeps the port
listening, so the client reconnects to the same URL.

Order watch unit 4 (2026-11-09): `tests/order_watch_process.rs` `mod operation` (10, `order_watch` against an isolated store and one
loopback relay: a JSON call returns the replay with a cursor and the next call with it returns only a record published while it waits,
stamped in the same second as the newest; a chat line from a clock 30 s behind and a node message stamped before a future-dated chat
line are not lost; a counterparty that a live node message names brings its chat replay into the same document and is not saved; the
end of the trade (`purchase-completed`, an end already in the replay, `canceled` for another order not counting) and `admin-settled`
ending the seller's watch but not the buyer's; an empty success at the deadline in both modes and a sink that cannot write; store
failures before any relay; a lookup failure's code; a frozen store that still watches, warning-free, and conforms to the generated
schema) and `parse_human::watch` (4).

Order watch unit 5 (2026-11-09): `tests/order_watch_process.rs` `mod binary` (6, the real binary in a sandbox with one loopback relay:
a human watch shows the replay, then a live node message and a chat line with ESC and a bidirectional control escaped, then ends on
`purchase-completed` with exit 0, empty stderr and no trade secret on either stream; `--timeout 1` prints the empty first update and the
deadline line, exit 0; a closed stdout exits 1 with one fixed line well before the deadline; on Unix, a reader that closes stdout after
the first update stops a watch with no deadline within 5 s (exit 1, the same line); `--since 0`, `--since 10081`, `--timeout 0` and a
malformed `--after` exit 2 with the fixed `invalid_arguments` line and `--after` with `--since` with the parser's conflict line, no relay
contacted and the value never echoed; one `--verbose` line; `--pow`, `--transport`, `POW` and `TRANSPORT` set to garbage change
nothing), `order.watch` in the registry mapping and in `every_legacy_command_rejects_json_before_any_effect` (27 tests in the file).
