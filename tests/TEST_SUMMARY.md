# Test Suite Summary

This document records current refactor evidence followed by the historical test
inventory. Historical totals and coverage descriptions are not verification of
the current checkout or the planned JSON pilot.

## Refactor Baseline Intake: 2026-10-16

**Scope**: T001 of the [order-list task list](../specs/001-order-list-json/tasks.md).
This is a documentation and source-inspection checkpoint, not a Cargo test run.

### Checkout Snapshot Before T001 Edits

Git status was checked in both repositories on native Windows using Git Bash.
Both remain on `develop`; no branch switch, staging, commit or remote operation
was performed during this intake.

| Repository | Observed working tree |
| --- | --- |
| mostro-cli | Seven modified, unstaged files below; no staged or untracked files |
| mcp | No staged changes; three untracked files: AGENTS.md, mostro-cli-mcp.code-workspace, plan-aiReadyMostroCli.prompt.md |

CLI files already modified at intake:

- [src/parser/dms.rs](../src/parser/dms.rs)
- [spec.md](../specs/001-order-list-json/spec.md)
- [plan.md](../specs/001-order-list-json/plan.md)
- [tasks.md](../specs/001-order-list-json/tasks.md)
- [research.md](../specs/001-order-list-json/research.md)
- [quickstart.md](../specs/001-order-list-json/quickstart.md)
- [contracts/cli.md](../specs/001-order-list-json/contracts/cli.md)

Preserve these changes. The feature-document edits include the approved A1
output-selection clarification and U1 generated-command-reference decision.
This snapshot predates the T001 summary and task-status edits themselves.

### Prior Evidence And Current Inspection

Earlier session results, also recorded in the
[cross-repository roadmap](../../mcp/plan-aiReadyMostroCli.prompt.md), are inherited
evidence only: native Windows `cargo test --locked` passed 151 tests, followed by
successful formatting, all-target/all-feature Clippy with warnings denied, and
debug build checks. After the earlier import cleanup, 18 parser unit cases,
formatting and Clippy were reported passing again. These results predate this
intake and later user edits; none has been rerun as part of T001.

Current source inspection of [dms.rs](../src/parser/dms.rs) confirms that
WaitingSellerToPay propagates a failed local status save and LastTradeIndex
rejects missing data/user or failed persistence before printing success. The
two waiting-seller cases, three trade-index persistence cases and missing-index
case remain present with in-memory failure injection. Presence is not runtime
verification; T004 must rerun them against the then-current checkout. No Rust
source was edited during this intake.

The earlier native Windows MCP test-compilation failure remains an unverified
baseline blocker: its stdio fixture uses Unix-only PermissionsExt/from_mode.
It was not rerun or repaired here. Linux runtime, declared MSRVs, the full
command/output/post-state matrix and CLI/core/daemon regtest compatibility remain
unverified. No live relays, trades, real identity databases or secrets were used.

### Intake Checks And Next Gate

- Spec Kit prerequisite discovery passed with `--json --require-tasks --include-tasks` and resolved the existing order-list feature; its feature identifier is not the Git branch name.
- The specification-quality checklist has 16 checked items and none unchecked. It is a requirements-review gate, not implementation acceptance.
- Existing ignore files cover the Cargo target directory, local environment/editor files, Spec Kit feature pointer and Python caches; no ignore-file change was needed for this documentation-only task.
- No extension hooks were found. Before editing, this historical summary already had 93 Markdown diagnostics; unrelated formatting is left unchanged.
- Next are T002 (production-size/import inventory) and T003 (listing characterization). T004-T010 still govern the baseline gate before pilot production work; no JSON command is implemented by T001.

## T002 And T003 Verification: 2026-10-16

**Scope**: Production-size/import inventory and existing listing behavior, not
the JSON pilot. See the [measured architecture inventory](../docs/architecture.md#production-inventory-2026-10-16)
for all 42 source files, five oversized modules and planned ownership splits.

Characterization reuses [parser_orders.rs](parser_orders.rs) and
[integration_tests.rs](integration_tests.rs): 38 parser cases and 12 integration
cases now pass, up from 11 and 4 respectively (35 additional cases).

| Behavior | Evidence |
| --- | --- |
| Latest state before status/side/currency filtering | Older matching events cannot resurrect an order whose newest state no longer matches |
| Duplicates and timestamp ties | Deduplicated event IDs; lowest event ID wins same-order equal-time ties; cross-order ties assert membership, not unstable order |
| Decoder permissiveness | Missing/invalid identity or side skips; absent status differs from invalid status; missing/bad numeric fields, partial/reversed ranges and signed i64 extremes retain existing behavior |
| Filters and defaults | Real command helper defaults to pending, accepts status case variants and rejects padding; reducer currency matching remains exact, including empty strings |
| Query construction | Injected UTC clock verifies the exact seven-day boundary; actual create_filter path verifies author, kind 38383, z=order, limit 50 and absence of extra fields such as until |
| Human output | Empty-book wording, ID, side, status, sats/market, currency, fixed/range fiat, joined payment method, premium and UTC event date/time |

Two behavior-preserving testability extractions were made:
`parse_status_filter` is used by the existing list command, and
`create_seven_days_filter_at` receives the clock while existing order/dispute
query paths continue to pass `Utc::now()`. No new command, dependency, transport,
storage effect or JSON interface was introduced. The DM parser's existing edits
were not changed. New fixtures use generated signing keys, fixed timestamps and
in-memory contexts without relay connections or identity initialization.

The first reducer characterization passed on existing behavior. Initial new
table assertions failed because the tests assumed a uniform border separator,
then because they treated colored cells as plain strings. Only those assertions
were corrected against the existing comfy-table preset and colored output;
these were test defects, not production bugs.

Executed on native Windows, host `x86_64-pc-windows-msvc`, with rustc 1.97.0
(2d8144b78) and Cargo 1.97.0 (c980f4866), from Git Bash:

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders --test integration_tests` | PASS: 38 + 12 = 50 cases |
| `cargo fmt --all -- --check` | PASS after formatting only the four touched Rust files |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 186 tests; no failed or ignored cases |
| `cargo build --locked` | PASS |

The full run includes the six existing DM persistence regression cases. At the
T003 handoff, T004 remained pending as the next explicit checkpoint; its fresh
focused results follow below. Linux, MSRV, MCP fixture migration, the complete command/state matrix and
regtest compatibility remain outside this increment. T010's baseline gate is
still open. No commits, public-node trades or real identity reads were performed.

## T004 Persistence Checkpoint: 2026-10-16

**Scope**: Rerun the existing WaitingSellerToPay and LastTradeIndex regressions
against the current checkout after the T003 handoff and subsequent user edits.
The current handlers and fixtures in [dms.rs](../src/parser/dms.rs) were inspected
before execution. All six cases passed without modifying Rust source or tests.

Executed on native Windows (`x86_64-pc-windows-msvc`) using rustc/Cargo 1.97.0
from Git Bash. The repository remains on `develop`, with existing unstaged
changes preserved. The fixtures use generated keys, in-memory SQLite and
synthetic user data; their clients are constructed without connecting to relays.
Save failures are injected with SQLite BEFORE UPDATE triggers. No real identity
database, private credentials or live trades are involved.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib parser::dms::tests::waiting_seller_to_pay_reports_persistence_result` | PASS: 2 cases, 0 failures |
| `cargo test --locked --lib parser::dms::tests::last_trade_index` | PASS: 4 cases, 0 failures |

| Regression case | Verified assertion |
| --- | --- |
| WaitingSellerToPay successful save | Returns success and stores waiting-payment status |
| WaitingSellerToPay rejected save | Returns an error retaining the injected cause; stored status stays pending |
| LastTradeIndex successful save | Returns success and changes the stored index from 3 to 7 |
| LastTradeIndex rejected save | Returns an error retaining the injected cause; stored index stays 3 |
| LastTradeIndex missing user | Returns an explicit local-user error instead of success |
| LastTradeIndex missing index | Returns an explicit trade-index error instead of success |

These are passing regressions for already-correct fixes, not newly reproduced
production defects. T004 does not alter the recent listing-test/query-helper
edits, add tests or repeat the historical fixes. Only evidence and task/plan
status are updated. The T003 full-suite/fmt/Clippy/build results remain historical
to that increment; they were not rerun or claimed as fresh checks for this
documentation-only checkpoint. Linux, MSRV and daemon compatibility remain
unverified. T005 is next and T010's baseline gate is still open.

## T005 New-Order Preflight: 2026-10-16

**Scope**: Bounded confirmation and unknown-market-fiat repair in
[new_order.rs](../src/cli/new_order.rs), tested through
[cli_functions.rs](cli_functions.rs). Started from clean CLI commit `443dc56`
on `develop`; no Git mutations, MCP changes or new dependencies were made.

The real handler now delegates to `confirm_new_order` and
`validate_market_fiat`. An initial behavior-preserving extraction exposed the
existing checks to an isolated subprocess: 6 of 11 cases failed because closed
stdin and an empty pipe were approved, while rejection, invalid input and unknown
fiat terminated with exit 0. Real LF/CRLF Enter, lowercase/uppercase `y`, and
known fiat already passed. After the repair the same 11 cases passed; they
passed again after tightening fixture cleanup and checking for filesystem effects.

| Input | Verified preflight result after repair |
| --- | --- |
| Closed stdin and an empty, closed pipe | Error containing confirmation context; child exits 1 without approval |
| Real blank Enter, both LF and CRLF | Returns success and reaches the approval marker |
| `y` and `Y` | Returns success and reaches the approval marker |
| `n` and `N` | Cancellation error; child exits 1 without approval |
| Unrecognized answer | Confirmation error; child exits 1 without approval |
| Known synthetic market fiat | Returns success |
| Unknown synthetic market fiat | Error retaining the explicit-amount `-a` hint; child exits 1 without approval |

The confirmation helper checks the zero-byte EOF result before interpreting the
answer. Both helpers return errors rather than terminating the process. Source
inspection confirms the handler calls them with `?` before publication,
`Commands::run` propagates the handler result, and `main` prints errors to stderr
and exits 1. The existing nonzero-amount bypass is unchanged. The production file
has 198 physical/production lines, below the 400-line limit.

The child is the existing integration-test executable running only its preflight
entrypoint, not the full CLI. It has an allow-listed environment (only necessary
Windows system paths plus fixture values), a unique temporary cwd/HOME/USERPROFILE/
MCLI_DIR, captured streams, a 40-second watchdog, kill/reap cleanup and no shell.
Each child leaves its fixture directory empty, and the parent removes that
directory after completion. The fiat list is in memory; no HTTP server, Yadio
request, Nostr client, real identity, invoice, DB or publication is involved.
This does not establish full-command startup, network or persistence behavior.

Executed on native Windows (`x86_64-pc-windows-msvc`) with rustc 1.97.0
(2d8144b78) and Cargo 1.97.0 (c980f4866), from Git Bash:

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions new_order_preflight_process` before repair | Expected RED: 6 failed, 5 passed |
| Same focused command immediately after repair and after fixture cleanup changes | PASS: 11 cases on each run |
| `cargo fmt --all -- --check` | PASS after formatting only the two touched Rust files |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 198 tests, including 42 CLI cases (11 new regressions and one child entrypoint); no failures or ignored cases |
| `cargo build --locked` | PASS |

No `--yes`, write JSON, retry, transport or persistence-model changes were added.
T001-T005 are complete; T006 follows below. Linux, MSRV, the complete command/state
matrix and daemon compatibility remain unverified; T010's baseline gate remains
open. The JSON pilot is still unimplemented.

## T006 Takesell Post-Accept Save: 2026-10-16

**Scope**: Spec Kit alignment for take-sell-with-invoice post-accept-save failure
coverage in [cli_functions.rs](cli_functions.rs) against
[take_order.rs](../src/cli/take_order.rs). The production defect (success after a
failed local save) was already fixed on `main` (#205); this increment does not
change the persistence model.

`save_taken_sell_order` and `accepted_take_local_save_failed` are public so the
integration tests can exercise the same path `execute_take_order` uses after a
correlated reply. Two synthetic in-memory cases refuse the order insert (`disk
full` trigger) or drop the `users` table before the index write. Each wraps the
save error with the accepted-take helper and asserts both the
`Mostro accepted the take of …` status and the original cause. No Nostr publish,
real invoice, retry or live node is involved. The book is unreachable (offline
client), so the helper saves id and trade keys only — the same post-accept path.

Focused regressions previously living only under `take_order`'s `#[cfg(test)]`
were moved into `cli_functions` to match the task path; successful-save unit
cases remain in `take_order.rs`.

Executed on native Linux (`x86_64-unknown-linux-gnu`) with rustc 1.97.0
(2d8144b78) and Cargo 1.97.0, after rebasing `develop` onto `origin/main`:

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions takesell_post_accept` | PASS: 2 cases |
| `cargo test --locked --lib take_order` | PASS: 5 remaining unit cases |
| `cargo fmt --all -- --check` | PASS after formatting the two touched Rust files |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 215 tests across lib and integration suites; no failures or ignored cases |
| `cargo build --locked` | PASS |

T001-T006 are complete; T007 is next. Linux/WSL regtest, MSRV and the full
command/state matrix remain unverified; T010's baseline gate remains open. The
JSON pilot is still unimplemented.

## T007 Unrelated-Kind-14 Reply Correlation: 2026-10-19

**Scope**: Prove that the reply wait in [messaging.rs](../src/util/messaging.rs)
skips an unrelated kind-14 reaching the trade key before the correlated reply,
through new coverage in [integration_tests.rs](integration_tests.rs). This is a
baseline-gate prerequisite, not JSON-pilot work.

The two wait paths were compared. The correlating path (`wait_for_dm_matching`
→ `wait_for_reply`, used by `getuserinfo`) keeps the REQ open for the full
window and filters each event by `is_mostro_reply` then
`event_matches_expected_reply` (decrypt → `message_matches_expected_reply`:
request_id first, then action allow-list). Its message-level predicates were
already unit-tested, but the event-level path (decrypt + parse) had no isolated
test. The per-event decision was extracted, behavior-preserving, into a public
`select_reply_event` helper (composing the existing predicates); `ExpectedReply`
gained a public `new` constructor. The wait loop now calls the extracted
helper, so the integration tests exercise the exact production decision.

Two synthetic tests build real NIP-44 kind-14 events (Mostro authors/signs and
addresses them to a generated trade key) and assert:

| Scenario | Result |
| --- | --- |
| Unrelated Mostro kind-14 (wrong request_id) arriving first | Skipped; correlated reply that follows is accepted |
| Same unrelated event under the non-correlating path (`expected = None`) | Accepted (documents why id-bearing flows must correlate) |
| NIP-17 peer chat from a stranger with a matching id | Skipped (not authored by Mostro) |
| Our own echoed request (admin-as-Mostro) | Skipped by event id; identical event accepted when not our echo |

The path is already correct, so these are passing characterization tests, not a
reproduced defect. No production behavior changed. No Nostr client, relay, real
identity or live node is involved; events are built in memory with generated
keys.

**Deferred finding (not a T007 change)**: the non-correlating `wait_for_dm`
closes after the first `is_mostro_reply` kind-14. If an unrelated Mostro kind-14
to the same trade key arrived first, `validate_response(request_id)` in
`print_dm_events` rejects it — a safe error, not a false success — and per-order
trade keys make the collision unlikely. Converting those id-bearing callers
(`take_order`, `new_order`, `add_invoice`, …) to `wait_for_dm_matching` is a
cross-command change that belongs with the later write-executor migration, so it
is recorded here rather than attempted now. End-to-end coverage of the live wait
loop (notification ordering, EOSE, subscription close) needs the SDK local-relay
harness built in T013/T015, also outside this baseline checkpoint.

Executed on native Linux (`x86_64-unknown-linux-gnu`) with rustc 1.97.0
(2d8144b78) and Cargo 1.97.0:

| Command | Result |
| --- | --- |
| `cargo test --locked --test integration_tests correlating_wait` | PASS: 2 cases |
| `cargo test --locked` | PASS: 217 tests (integration 12 → 14); no failures or ignored cases |
| `cargo fmt --all -- --check` | PASS after formatting the two touched Rust files |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --locked` | PASS |

T001-T007 are complete; T008 is next. Linux/WSL regtest, MSRV and the full
command/state matrix remain unverified; T010's baseline gate remains open. The
JSON pilot is still unimplemented.

## T008 Existing-Command State Matrix: 2026-10-19

**Scope**: Catalogue every existing `Commands` variant against the
success/failure/human-output/post-state dimensions and mark each unverified cell
explicitly, as a baseline-gate input for T010. Documentation checkpoint — no
code changed; the evidence basis is the T007 run (`cargo test --locked`, 217
passing). No live node, real identity or relay was used.

**Inventory source**: `docs/commands.md` plus the authoritative Clap `Commands`
enum in `src/cli.rs` (33 variants). The roadmap inventory
`../mcp/plan-aiReadyMostroCli.prompt.md` is **absent on this Linux checkout** and
could not be cross-referenced; recorded as a known limitation. The enum includes
`addbondinvoice`, `admsetmaintenance` and `admmaintenancestatus`, which are not
yet in `docs/commands.md` (doc drift to fix in T049).

**Legend**
- `F` — isolated-fixture evidence (in-memory SQLite, synthetic signed events,
  subprocess, or parser fixtures).
- `P` — payload/parse-only: message construction or argument parsing is
  verified, but the full request→reply flow is not exercised.
- `RO` — read-only command; no local writes, covered by pure parser/reducer
  fixtures.
- `—` — unverified: no isolated evidence yet (needs the T013 process/transport
  harness).

| Command | Success | Failure | Human output | Post-state | Evidence |
| --- | --- | --- | --- | --- | --- |
| `listorders` | F | F (status validation) | F | RO | `parser_orders.rs` (38); `integration_tests.rs` listing/filter |
| `neworder` | P + F (preflight) | F (closed-stdin/EOF/`n`/unknown/unknown-fiat) | — | F (no publish/fs effect pre-approval) | `cli_functions.rs` `new_order_preflight_process`, `test_new_order_message_with_trade_index` |
| `takesell` | P + F (save) | F (post-accept save order/index failure) | — | F | `cli_functions.rs` `takesell_post_accept_*`, take-sell payloads; lib `take_order` |
| `takebuy` | P | — | — | P (shared `save_order` path) | `cli_functions.rs` `test_take_buy_payload_with_amount`; lib `take_order` |
| `addinvoice` | — | — | — | — | none |
| `addbondinvoice` | P (no-reply=success logic) | P (`WaitForDmTimeout` vs `PowRequirementUnmet`) | — | — | lib `messaging` `pow_requirement_unmet_*` |
| `fiatsent` | P | — | — | — | `cli_functions.rs` `test_send_msg_fiat_sent_action` |
| `release` | P | — | — | — | `cli_functions.rs` `test_send_msg_release_action` |
| `cancel` | P | — | — | — | `cli_functions.rs` `test_send_msg_cancel_action` |
| `ordersinfo` | P | F (empty order-ids) | — | — | `cli_functions.rs` `test_orders_info_*` |
| `dispute` | P | — | — | — | `cli_functions.rs` `test_send_msg_dispute_action` |
| `listdisputes` | F | — | F | RO | `parser_disputes.rs` (9) |
| `getdm` | F (parse) | — | F | RO | `parser_dms.rs` (19) |
| `getdmuser` | F (kind-14 parse) | — | F | RO | `parser_dms.rs` `parse_dm_events_accepts_wrap_message_output` |
| `senddm` | P | — | — | — | `cli_functions.rs` `test_dm_message_creation` |
| `dmtouser` | P (wrap/parse) | — | — | — | lib `messaging` roundtrip; `parser_dms.rs` accepts-wrap |
| `disputechat` | P (shared kind-14 wrap/parse) | — | — | — | lib `messaging` roundtrip; `parser_dms.rs` |
| `getadmindm` | F (parse) | — | F | RO | `parser_dms.rs` (shared `execute_get_dm` display) |
| `admsenddm` | — | — | — | — | none |
| `sendadmindmattach` | — | — | — | — | none |
| `conversationkey` | — | — | — | — | none |
| `getlasttradeindex` | P | F (reply persistence failure) | — | F | `cli_functions.rs` `test_last_trade_index_message`; lib `parser::dms` `last_trade_index` (4) |
| `getlasttradeprivkey` | — | — | — | — | none |
| `getuserinfo` | F (id/action correlation, first-contact PoW) | F (PoW unmet, uncorrelated skip) | — | — | `integration_tests.rs` `correlating_wait_*`; lib `messaging` correlation + PoW |
| `restore` | P | — | P (payload display) | — (known gap, issue #188) | `cli_functions.rs` restore msgs; `parser_dms.rs` restore payload |
| `admcancel` | P | — | — | — | `cli_functions.rs` `test_dispute_message_cancel` |
| `admsettle` | P | — | — | — | `cli_functions.rs` `test_dispute_message_settle` |
| `admcancelpending` | F (pre-context RPC routing) | — | — | — | `cli_functions.rs` `rpc_commands_are_routed_before_the_nostr_context` |
| `admaddsolver` | P + F (own-echo skip) | — | — | — | `cli_functions.rs` `test_dispute_message_creation_add_solver`; lib `messaging` own-echo skip |
| `admtakedispute` | P | — | — | — | `cli_functions.rs` `test_dispute_message_take` |
| `admsetmaintenance` | F (parse + routing) | F (requires `enabled` value) | — | — | `cli_functions.rs` `admsetmaintenance_*`, RPC routing |
| `admmaintenancestatus` | F (parse + routing) | — | — | — | `cli_functions.rs` `admmaintenancestatus_parses_without_arguments` |
| `rate` | P | F (rating out of range) | — | — | `cli_functions.rs` rating/payload tests |

**Findings for the T010 gate**
1. **Systemic gap**: there is no fake-transport harness yet (that is T011-phase
   T013), so end-to-end request→reply **success** and relay/read **failure** are
   unverified for essentially every mutating and RPC command. Coverage today is
   strong at the payload/parse and parser/human-output layers, thin at the
   full-flow layer.
2. **Human output** is fixture-verified only for the read/display commands
   (`listorders`, `listdisputes`, `getdm`/`getdmuser`/`getadmindm`); mutating
   commands' reply rendering is unverified.
3. **Post-state** is fixture-verified only where isolated SQLite fixtures exist:
   takesell post-accept save (success and failure), last-trade-index persistence
   (via the DM handler) and new-order preflight no-publish/no-fs-effect.
4. **Known defect**: `restore` does not persist recovered orders (issue #188);
   its post-state remains an open gap, not a verified row.
5. **Doc drift**: `docs/commands.md` omits `addbondinvoice`,
   `admsetmaintenance`, `admmaintenancestatus`; reconcile in T049.

T001-T008 are complete; T009 is next (native Windows/Linux baseline, declared
MSRV and CLI/core/daemon compatibility). The matrix above shows most
mutating-command rows stay explicitly unverified until the T013 harness lands;
T010's baseline gate remains open and the JSON pilot is unimplemented.

## T009 Platform, MSRV And Compatibility Baseline: 2026-10-20

**Scope**: Record the native platform baseline, the declared-MSRV check and a
CLI/core/daemon compatibility check, as a T010 input. No Rust source, MCP code,
dependency or MSRV declaration was changed.

### Linux Baseline (Pinned Toolchain)

Native Linux (`x86_64-unknown-linux-gnu`), rustc/Cargo 1.97.0 from
`rust-toolchain.toml`, on the T008 checkout (`66bf573`):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 217 tests; no failures or ignored cases |
| `cargo build --locked` | PASS |

### Windows Baseline

**Blocked**: no native Windows host is available in this environment. The
earlier Windows results (T001-T005, `x86_64-pc-windows-msvc`) predate T006-T008
and are not a current Windows baseline. The known MCP Windows test-compilation
blocker (Unix-only `PermissionsExt` in its stdio fixture) remains unverified.

### Declared MSRV

`Cargo.toml` declares `rust-version = "1.86"`. The locked dependency graph does
not support it:

| Command | Result |
| --- | --- |
| `cargo +1.86.0 check --locked --all-targets --all-features` | **FAIL** at dependency resolution |
| `cargo +1.93.0 check --locked --all-targets --all-features` | **FAIL** at dependency resolution |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Packages that reject 1.86.0: `sqlx`, `sqlx-core`, `sqlx-macros`,
`sqlx-macros-core` and `sqlx-sqlite` 0.9.0 (require 1.94.0); dev-dependencies
`serial_test`/`serial_test_derive` 4.0.1 (require 1.93.1); `tonic`/`tonic-prost`
0.14.6 (require 1.88). The effective minimum of the locked graph is therefore
**1.94.0**. The declaration was not changed and no dependency was downgraded;
whether to raise `rust-version` or pin older dependencies is an open maintainer
decision, not something this checkpoint may do to make the check pass.

### CLI/Core/Daemon Compatibility (Isolated Regtest)

Stack: `../mcp/regtest/compose.yml` (mostrod **v0.19.3** checksum-verified
release binary, LND 0.18.3, bitcoind 27.0, nostr-rs-relay 0.9.0) started with
`devenv.sh up` from a fresh state (no prior containers, volumes or `.state`).
CLI: this checkout's debug build (mostro-core `=0.17.3`), run as the throwaway
regtest user `alice` via `devenv.sh cli` with `MCLI_DIR` under the regtest
`.state` directory and closed stdin except where an answer was piped. Every key
and coin is a regtest fixture; no public relay, mainnet node or real identity
was touched.

| Step | Command | Observed result |
| --- | --- | --- |
| 1 | `-v listorders` | Exit 0; relay connected; `Transport: nip44 (auto-detected protocol v2)`; empty-book table. First run created `mcli.db` and a new identity. |
| 2 | `neworder -k sell -a 10000 -c USD -f 10 -m bank` (stdin `y`) | Exit 0; mostrod replied with the new order `ab8f4f36-…`; status Pending; trade index 1; order saved locally |
| 3 | `listorders` | Exit 0; the order listed as `sell`/`pending`/10000 sats/USD 10/bank |
| 4 | `cancel -o ab8f4f36-…` | Exit 0; mostrod confirmed `Canceled`; local row deleted, as `Action::Canceled` handling intends |
| 5 | `listorders` / `listorders -s canceled` | Pending book empty; order listed as `canceled` |
| Post-state | `sqlite3` on alice's `mcli.db` | `orders` empty; `users.last_trade_index` = 1 |

This shows the current CLI and mostro-core 0.17.3 interoperate with mostrod
v0.19.3 for transport auto-detection, public listing, order creation, request/
reply correlation and cancellation. It does **not** cover take/invoice/payment,
dispute, admin, RPC or restore flows; those remain unverified against a daemon.
The stack was then stopped with `devenv.sh down` (containers removed, volumes and
regtest identities kept); no `clean`/volume wipe was performed. The MCP
repository's working tree is unchanged.

**Pilot-relevant observation**: today's `listorders` initializes the local
database and creates an identity on first use (step 1). The pilot requires
listing and discovery to run without identity or DB creation (FR-006/007,
SC-003), so this is baseline behavior the pilot must change, not a defect fixed
here.

**Additional doc drift for T049**: actual flags are `--fiatcode`,
`--fiatamount`, `--paymentmethod` and `--orderid`, while `docs/commands.md`
documents `--fiat-code`, `--fiat-amount`, `--payment-method` and `--order-id`.

**MSRV follow-up (maintainer decision, 2026-10-20)**: the declared minimum was
raised to `rust-version = "1.94"` to match the locked graph; no dependency
changed and `Cargo.lock` is unchanged. The CI `msrv` job previously ran the
1.97.0 development toolchain (and `rust-toolchain.toml` would override an
installed toolchain anyway); it now runs `cargo +1.94.0 build --locked
--all-features`. Re-verified on native Linux:

| Command | Result |
| --- | --- |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |
| `cargo +1.94.0 build --locked --all-features` (CI command) | PASS |
| `cargo +1.93.0 check --locked --all-targets --all-features` | Rejected: `mostro-cli@0.16.3 requires rustc 1.94` |
| `cargo test --locked` (1.97.0) | PASS: 217 tests |

**Open items carried to T010**: native Windows baseline (blocked, no host);
daemon compatibility verified only for the listing/create/cancel subset. The
MSRV item is resolved by the follow-up above.

T001-T009 are complete; T010 (baseline-gate review) is next. The JSON pilot is
still unimplemented.

## T010 Baseline Gate Decision: 2026-10-20

**Decision: GO for pilot foundations (T011+), with the recorded exceptions
below.** This closes the inherited Phase-1 baseline gate for the read-only
order-list pilot only; it is not acceptance of the pilot or of any write flow.

**Limitation**: the roadmap's own Phase-1 gate text
(`../mcp/plan-aiReadyMostroCli.prompt.md`) is absent on this checkout, so the
review uses the criteria restated in T010 and the constitution.

| Criterion | Evidence | Status |
| --- | --- | --- |
| Persistence regressions (WaitingSellerToPay, LastTradeIndex) | T004: 6 cases pass | Met |
| Closed-stdin / EOF confirmation | T005: 11 cases, red before fix, green after | Met |
| Take-with-invoice post-accept save | T006: 2 failure cases + save unit cases | Met |
| Correlated reply wait | T007: unrelated kind-14 skipped; non-correlating `wait_for_dm` finding deferred to the write-executor migration | Met |
| Size/import inventory | T002, `docs/architecture.md` | Met |
| Command/state matrix | T008: all 33 commands, unverified cells explicit | Met as a record; full-flow rows stay unverified until T013 |
| Linux baseline, pinned 1.97.0 | T009 and below | Met |
| Declared MSRV | Raised to 1.94; 1.94.0 check passes, CI now tests it | Met |
| CLI/core/daemon compatibility | T009 regtest vs mostrod v0.19.3: listing, create, cancel | Met for the pilot's read path; other flows unverified |
| Native Windows baseline | No Windows host available | **Deferred** (user decision) to T051-T052 |

Code gates on `96aeb77`, native Linux, rustc 1.97.0:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 217 tests, 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

**Recorded exceptions carried forward**
1. Native Windows baseline is deferred by user decision; it must be run before
   pilot acceptance in T051-T052 and stays a blocker for SC-005.
2. Daemon compatibility covers listing, order creation and cancellation only;
   take/invoice/payment/dispute/admin/RPC/restore remain unverified against a
   daemon and are outside this read-only pilot.
3. End-to-end success/failure/post-state for mutating commands remains
   unverified until the T013 process/transport harness exists.
4. `restore` post-state is a known defect (issue #188), outside pilot scope.
5. `listorders` currently creates a DB and identity on first use; the pilot
   must remove this (FR-006/007, SC-003).

T001-T010 are complete and the baseline gate is closed for the pilot. T011
(split `src/cli.rs`) is next. The JSON pilot is still unimplemented.

## T011 CLI Module Split: 2026-10-20

The 810-line production section of `src/cli.rs` was split without behavior
change. Native Linux, rustc 1.97.0, base `c2a745e`.

| File | Owns | Production lines |
| --- | --- | ---: |
| `src/cli.rs` | Facade: subcommand modules, `Cli`/`Commands`/`Context` re-exports, `run()` | 57 |
| `src/cli/args.rs` | `Cli` and the fiat-range parser | 75 |
| `src/cli/commands.rs` | `Commands` enum (33 variants) | 300 |
| `src/cli/context.rs` | `Context`, env mirroring, config validation, `init_context`, transport detection, admin check | 214 |
| `src/cli/legacy.rs` | `Commands::run_rpc` and `Commands::run` dispatch | 197 |

Moved code was compared line by line, in order, against `HEAD:src/cli.rs`
after mapping new top-of-file imports back to the old inline paths. The only
layout difference is one `ADMIN_NSEC` `map_err` call that rustfmt now keeps on
one line. Inline paths, the `take_dispute::*` glob and the core/SDK preludes
became explicit imports. External paths `crate::cli::{Cli, Commands, Context,
run}` are unchanged, and the three setup-message unit tests moved with their
code to `cli::context::tests`.

| Check | Result |
| --- | --- |
| `cargo test --locked --test cli_functions` after each of the four extractions | PASS: 44 each time |
| Root and all 33 subcommand `--help` outputs, plus `--version` | Byte-identical before/after |
| `listorders` with empty `HOME` and no config | Exit 1 with `MOSTRO_PUBKEY not set.`; no file created |
| `admmaintenancestatus` with no Nostr config | Routed to admin RPC before context (connect error, no config error) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 217 tests, 0 failed (unchanged) |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-features` | PASS |

No characterization test was added: existing RPC-routing, parsing and setup
tests already pin the moved behavior, and none fail before the split. Four
legacy files still exceed 400 production lines (`db.rs`, `parser/dms.rs`,
`util/messaging.rs`, `cli/take_dispute.rs`); none was touched.

T001-T011 are complete. T012 (Serde/schemars dependencies) is next. The JSON
pilot is still unimplemented.

## T012 Dependencies: 2026-10-20

Manifest changes only; no Rust source uses the new crates yet. Native Linux,
rustc 1.97.0, base `3a53e90`.

| Change | Why |
| --- | --- |
| `serde` gains explicit `derive` | Derive previously arrived only through feature unification |
| `schemars = 1.2.3`, defaults off, `derive` + `std` | Generate JSON Schemas from DTOs (research section 6) |
| Dev-only `nostr-sdk` feature `local-relay` | In-process `LocalRelay`/`MockRelay` for T013-T015: empty relay (EOSE), rejecting `QueryPolicy` (CLOSED), `unresponsive_connection` (silent), `shutdown` (disconnect) |

`Cargo.lock` was updated by a plain `cargo check`, not `cargo update`. It adds
exactly 8 packages and changes or removes none: btreecap 0.1.0, dyn-clone
1.0.20, nostr-memory 0.45.2, ref-cast 1.0.27, ref-cast-impl 1.0.27, schemars
1.2.3, schemars_derive 1.2.3, serde_derive_internals 0.30.0. Their highest
declared minimum Rust is 1.85 (nostr-memory), below the project's 1.94.

| Check | Result |
| --- | --- |
| `cargo tree -e normal,build -i nostr-memory` | Nothing: the fixture crate is absent from the shipped graph |
| Production nostr-sdk features | `default`, `ring`, `rustls-tls-webpki-roots` (unchanged) |
| Dev nostr-sdk features | Same plus `local-relay` |
| schemars features | `derive`, `std` only |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 217 tests, 0 failed (unchanged) |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS (compiles the dev fixture graph) |

The fixtures are compiled but not yet exercised; T013 builds the harness and
T015 proves each notification case.

T001-T012 are complete. T013 (portable process harness) is next. The JSON pilot
is still unimplemented.

## T013 Process Harness: 2026-10-21

New target `tests/order_list_process.rs`. Native Linux, rustc 1.97.0, base
`7bd66d7`. Windows is not run (deferred to T051-T052).

| Requirement | How the harness meets it |
| --- | --- |
| Generated keys | `Keys::generate()` for the Mostro pubkey and seeded events |
| Loopback, OS-assigned port | Harness binds `127.0.0.1:0` itself and keeps the listener |
| Readiness signal | The listener is bound when `LoopbackRelay::start` returns; no sleep or polling |
| SDK fixtures, no handwritten framing | hyper WebSocket upgrade, tungstenite `derive_accept_key`, then `LocalRelay::take_connection` |
| Allow-listed environment | `env_clear()`, then only `SYSTEMROOT` (when present) plus sandbox variables and the test's node config |
| Isolated HOME/USERPROFILE/MCLI_DIR/cwd | Per-test directory under the system temp dir, created fresh and removed on drop, including on panic |
| Closed stdin, captured streams | `Stdio::null()` stdin; stdout and stderr piped and read concurrently |
| 40-second watchdog, guaranteed reap | `timeout` around `wait()`; on expiry `kill()` (which also waits); `kill_on_drop` as a fallback |
| Relay cleanup | Dropping `LoopbackRelay` aborts its accept task and all sessions and shuts the relay down |

**Design decision (user, 2026-10-21)**: nostr-sdk's `LocalRelay` picks its own
port by probing a random port, dropping the probe and binding again, which is the
guessed-port race the quickstart forbids. Its test option for an unresponsive
relay is crate-private. The harness therefore owns the listener and adds
dev-only hyper 1.8.1 (`server`, `http1`), hyper-util 0.1.20 (`tokio`) and
tungstenite 0.28.0 (`handshake`). All three were already locked; `Cargo.lock`
gains only three entries in mostro-cli's own dependency list, with no package
added or changed. Production-graph features of all three crates are identical
before and after. A `Silent` backend (upgrade completed, never answered) stands
in for the private unresponsive option.

| Test | Proves |
| --- | --- |
| `child_environment_is_allow_listed_and_sandboxed` | Child variables are only the allow list; sandbox paths and cwd are inside the sandbox |
| `sandbox_is_removed_on_drop` | Sandbox directories and their files are deleted |
| `parent_config_does_not_leak_into_the_child` | Exit 1 `MOSTRO_PUBKEY not set`, empty stdout, no files in home/mcli/cwd |
| `loopback_relay_serves_sdk_clients_on_an_os_assigned_port` | Nonzero loopback port; an SDK client fetches exactly the seeded event over one connection |
| `child_reaches_the_loopback_relay_with_sandboxed_storage` | `listorders` exits 0 against the loopback relay; HOME untouched |
| `watchdog_kills_and_reaps_a_hung_child` | Against a silent relay, a 2-second limit kills the child and leaves no running process |

Checks that the tests are not vacuous:
- With `MOSTRO_PUBKEY`, `RELAYS`, `MOSTRO_RPC_TOKEN` and `ADMIN_NSEC` planted in
  the parent, the leak test still passes.
- With `env_clear()` removed (temporary mutation, reverted), the same test fails:
  the child used the parent's config instead of failing on missing config.
- 10 consecutive runs of the target: 10/10 pass, about 2 s each. No
  `mcli-process-*` directory remained in the temp dir.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process` | PASS: 6 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 223 tests (217 + 6), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Limitations:
- `child_reaches_the_loopback_relay_with_sandboxed_storage` runs today's
  `listorders`, which still creates a database in the sandbox `MCLI_DIR`. The
  test asserts only that HOME stays untouched, so it keeps passing when the pilot
  removes that side effect.
- On Windows, `dirs::home_dir` uses the known-folder API rather than
  `USERPROFILE`, so `MCLI_DIR` is the variable that actually isolates storage
  there. Native Windows runs remain T051-T052.
- EOSE-empty, CLOSED, disconnect and silent-relay observability belongs to
  T015. This harness only provides the fixtures.

T001-T013 are complete. T014 (adversarial output-intent tests) is next. The
JSON pilot is still unimplemented.

## T014 Output-Intent Recovery: 2026-10-21

New module `src/cli/invocation.rs` (236 lines) and the `output_intent` module
in `tests/cli_functions.rs`. Native Linux, rustc 1.97.0, Clap 4.6.7, base
`216cb95`, code commit `95b6633`.

`parse_invocation(command, argv)` lets Clap parse first. Clap decides help and
version (`DisplayHelp`/`DisplayVersion` become `Text`); every other result is
either `Parsed` with the bound output mode or `Rejected` with a recovered
`ArgumentFailure { output, command }`. Recovery walks argv using only Clap
metadata: long names and aliases, shorts, `get_num_args`,
`allow_hyphen_values`, `allow_negative_numbers` and subcommand names. It never
strips tokens, never runs a command, and treats option values as values. The
helper is not wired into `run()`; T017 integrates it.

The tests run against `Cli::command()` plus a fixture: a global `--output`
(`human|json`) and an `order list` subcommand with `--side`, `--currency` and
`--status`. The real pilot grammar arrives in T017.

| Test | Proves |
| --- | --- |
| `contract_decision_table_rows` | All 12 malformed-argv rows of [the CLI contract](../specs/001-order-list-json/contracts/cli.md#streams-and-exit-codes): output selection, `invalid_arguments`/exit 2, resolved or null command |
| `output_is_selected_at_every_depth_and_defaults_to_human` | Root, intermediate, leaf and `=` forms; no declaration means human |
| `values_resembling_flags_or_commands_are_not_declarations` | `-m=--output=json`, `--message=--output=json` and variadic `senddm -m order list` stay values; root `-m order` hides `list`; `-m --output=json` is a declaration |
| `arities_and_hyphen_values_follow_clap` | Hyphen-allowed `--premium` takes `-5` and swallows `--output`; missing values at root (`--pow`) and leaf (`--currency`, `--side`) leave the declaration standing |
| `short_clusters_and_colliding_shorts_follow_scope` | `-vs`, `-vm <key>`, `-vm--output=json`, `-m=--output=json`, unknown `-vx`; leaf `-m` (message) is not root `-m` (pubkey) |
| `duplicate_and_missing_declarations_are_invalid` | Identical same-level, `=` form and root plus intermediate duplicates; missing, empty and flag-followed values |
| `terminator_ends_option_parsing` | `--output json` after `--` is not a declaration |
| `clap_decides_help_and_version_precedence` | `--help` anywhere is text with exit 0; `--output bogus --help` is an argument error because Clap says so |
| `non_unicode_values_keep_json_intent` (Unix) | Invalid UTF-8 values are rejected without losing JSON intent |
| `failures_carry_no_argument_text` | Failures carry only the mode and command names, never argument values |

Clap 4.6.7 behaviour found by direct probes before writing the tests:
- `--output` at two levels parses in Clap, but same-level duplicates fail with
  `ArgumentConflict`. The helper rejects both, because the contract forbids any
  duplicate declaration.
- `--message=a b` ends the occurrence at the attached value. `-m=hello` and
  `-mhello` both bind `hello`.
- A missing value is `InvalidValue`; a flag-looking next token is not consumed,
  except by `allow_hyphen_values` options such as `--premium`, which take
  `--output` as their value.
- A variadic option takes command-looking words but stops at flags.
- `--output json order` gives `DisplayHelpOnMissingArgumentOrSubcommand`, which
  is an argument error here, not the help exception.
- `-c -5` is `UnknownArgument` (negative numbers are not allowed there).

Checks that the tests are not vacuous:
- Red run with a stub that always returned human and no command: 9 of 10
  failed (`failures_carry_no_argument_text` passed trivially).
- Three temporary mutations, each reverted: ignoring `allow_hyphen_values`
  failed 1 test; continuing past `--` failed 2; accepting the first of several
  declarations failed 2.
- For every `Parsed` case the test compares the helper's mode with the value
  Clap actually bound.

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions output_intent` | PASS: 10 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 233 tests (223 + 10), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Limitations:
- Recovery is checked against a fixture grammar; T017 must rerun these tests
  against the real `order list` and global `--output`.
- Exit codes and stream content are asserted at the helper level
  (`ARGUMENT_ERROR_EXIT`, `json_error_code`); process-level streams are covered
  when T017 wires the helper and later process tests use the T013 harness.

T001-T014 are complete. T015 (SDK notification observability) is next. The
JSON pilot is still unimplemented.

## T015 Notification Observability: 2026-10-21

Three tests added to `tests/order_list_process.rs` (9 in total). Native
Linux, rustc 1.97.0, nostr-sdk 0.45.4, base `3bdb671`, test commit `828dbdb`.
The result and its design constraints are recorded in
[research §2](../specs/001-order-list-json/research.md#2-empty-results-and-completion).
No production code changed.

| Fixture | How it is built |
| --- | --- |
| EOSE-empty | Default `LocalRelay` with no events |
| CLOSED | `LocalRelay` whose `QueryPolicy` rejects every REQ with prefix `restricted` and message `queries disabled` |
| Silent | `Backend::Silent`: WebSocket accepted, never answered |
| Disconnect | `LocalRelay` whose `QueryPolicy` signals the REQ and never answers; the test then drops the endpoint and its connections |

| Test | Proves |
| --- | --- |
| `convenience_fetch_reports_success_for_every_failure` | `fetch_events` returns `Ok` with no events for all four fixtures |
| `explicit_subscription_distinguishes_completion_per_relay` | One explicit subscription over two seeded relays (same event), EOSE-empty, CLOSED and silent relays: EOSE with the event from both seeded relays, EOSE empty, CLOSED with its message, no signal; `Output::success` lists all five |
| `disconnect_before_eose_is_observed_per_relay` | The relay's own stream reports the disconnect before any EOSE |

Measured `fetch_events` times with a 2-second timeout (temporary probe,
reverted): EOSE-empty 0.7 ms, CLOSED 0.5 ms, disconnect 0.5 ms, silent 2.0 s.
All four returned `Ok(0 events)`.

Checks that the tests are not vacuous:
- Each failure fixture is the same in both tests, and the subscription test
  shows each one produces a different signal, so the fetch result hides a
  real difference.
- The disconnect test cannot pass unless the relay actually received the
  REQ: the endpoint is dropped only on that signal, and the test would hang
  otherwise.
- Counting events from `RelayNotification::Event` instead of raw
  `RelayMessage::Event` (temporary mutation, reverted) made one seeded relay
  report EOSE with no events.
- 10 consecutive runs of the target: 10/10 pass, about 2 s each. No sandbox
  directory remained.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process` | PASS: 9 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 236 tests (233 + 3), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Limitations:
- SDK-level only. No production collection exists yet; the CLI child is not
  exercised against these fixtures until the pilot adapter is built.
- Reconnect and resubscription behaviour after a disconnect was not tested.
- Windows runs remain T051-T052.

T001-T015 are complete. T016 is next. The JSON pilot is still unimplemented.

## T016 Outcome Envelope: 2026-10-21

New module `src/outcome.rs` (254 production lines, plus its `#[cfg(test)]`
module). Native Linux, rustc 1.97.0, base `186978d`, code commit `c94f1fb`.
Nothing calls it yet; T018 wires it to the output boundary.

Design:
- `Envelope<D, S>` has private fields. `Envelope::ok(command, data, ...)` and
  `Envelope::error(command_or_null, code, ...)` are the only constructors, so
  status, data and error cannot conflict, and neither emits `uncertain`.
  `D` and `S` stay generic until ListData, Snapshot and DiscoveryData land
  (T022, T026, T042).
- No field is skipped when empty; null keys and the `[]` warnings array are
  always serialized.
- `ErrorInfo` and `Warning` take their message from the code, so no
  caller-provided text, raw error chain, argument or URL can reach the wire.
  `ErrorCode::exit_code` maps argument codes to 2 and all others to 1.
- `Warnings::add` keeps one warning per code, which aggregates repeated
  invalid timestamps.
- `ExecutionMeta::read` sets all three stages to `not_applicable`;
  `ExecutionMeta::not_executed` sets `not_attempted`/`unknown`/`not_attempted`
  with a null snapshot. The full roadmap stage enums and `Status::Uncertain`
  exist for schema reservation only.

| Test | Proves |
| --- | --- |
| `success_examples_are_reproduced` | `large_amount_success` and `empty_success` from [examples.json](../specs/001-order-list-json/contracts/examples.json) serialize identically, exit 0 |
| `error_examples_are_reproduced` | `partial_relay_timeout`, `invalid_arguments` and `unsupported_secret_command` serialize identically with their exits |
| `every_key_is_present_and_null_when_empty` | All 7 envelope keys and 4 meta keys present; null command, data, error and snapshot; `schema_version` is the string `"1"` |
| `status_data_and_error_always_agree` | Every error code yields status `error`, data null, a `{code, message}` object and the contract exit |
| `error_codes_and_messages_follow_the_contract` | The 9 codes serialize to the contract names; messages are distinct, ASCII, sentence-shaped and free of URLs, braces and secret markers |
| `warnings_aggregate_by_code_and_never_serialize_as_null` | Empty warnings serialize as `[]`; repeated codes aggregate; a cleanup warning keeps the original error |
| `stages_follow_the_data_model` | Read and not-executed stage combinations |
| `reserved_stage_sets_match_the_roadmap` | Every publication, daemon-acceptance and persistence value serializes to its roadmap name |

The contract-name tests use exhaustive `match` tables, so a new code or stage
fails to compile until its contract name is added.

Checks that the tests are not vacuous:
- Red run with `todo!()` constructors: 7 of 8 failed.
  `reserved_stage_sets_match_the_roadmap` passed, since it checks only derived
  serde names.
- Two temporary mutations, each reverted: omitting a null snapshot failed 3
  tests; removing warning aggregation failed 1.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib outcome` | PASS: 8 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 244 tests (236 + 8), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Limitations:
- `src/cli/invocation.rs` still defines its own `ARGUMENT_ERROR_EXIT` and
  `invalid_arguments` string; T017 should reuse the outcome constants when it
  integrates the helper.
- Canonical command names are not validated here; the T018 registry supplies
  them.
- Messages for codes without a contract example are proposed prose. Only the
  codes are stable for matching.

T001-T016 are complete. T017 (fallible parsing and output selection) is next.
The JSON pilot is still unimplemented.

## T017 Fallible Parsing And Output Selection: 2026-10-21

Changed `src/cli/args.rs`, `src/cli/invocation.rs` and `src/cli.rs`; tests in
`tests/cli_functions.rs` and `tests/order_list_process.rs`. Native Linux,
rustc 1.97.0, base `04b319f`, code commit `bdf0685`.

Behaviour of the real binary:
- `Cli` declares `--output <human|json>`: global, long only (no colliding
  short), case-sensitive, default human. Every subcommand accepts it.
- `run()` parses `env::args_os()` through T014's `parse_invocation` instead of
  `Cli::parse()`, before the crypto provider, RPC routing or `init_context`.
- Clap's own help/version result prints text and exits 0, also with
  `--output json`. A bare `mostro-cli` still prints help on stderr with exit 2.
- Argument errors exit 2 with fixed stderr: `error: <Clap's fixed kind
  description>` plus a `--help` hint. Clap's message, which echoes tokens such
  as an unknown flag, is never printed.
- A missing, invalid or repeated `--output` exits 2 with empty stdout and
  `error: --output takes `human` or `json` and may be given only once`.
- `--output json` is rejected with exit 2 before any effect, both for valid
  legacy invocations and for argument errors. T018 replaces these stderr
  rejections with `output_not_supported`/`invalid_arguments` envelopes once the
  canonical-name registry exists.
- `invocation.rs` now takes exit and error codes from `outcome`
  (`json_error_code` returns `ErrorCode`), as T016 noted.

| Test | Proves |
| --- | --- |
| `real_cli_declares_a_global_case_sensitive_output` (unit) | The real `Cli` declares `--output` globally with values human/json, no short, no ignore-case; every subcommand except Clap's generated `help` accepts it; root and leaf positions bind; `JSON` is invalid |
| `rejections_keep_the_clap_kind_and_a_safe_diagnostic` (unit) | Unknown flag, unknown subcommand, invalid and duplicate `--output` keep Clap's kind; diagnostics start with `error: `, name the problem and omit the sentinel |
| `argument_errors_exit_2_with_safe_stderr_before_any_effect` | Five human-mode and invalid-selection cases: exit 2, empty stdout, sentinel-free stderr, no files, zero relay connections |
| `json_intent_never_runs_a_legacy_command` | `--output json listorders`, `listorders --output=json` and JSON with an unknown command exit 2 with no files and zero connections |
| `help_and_version_stay_text_on_stdout` | Five help/version forms, including with `--output json`: exit 0, text on stdout, empty stderr |
| `bare_invocation_still_prints_help_on_stderr` | No arguments: exit 2, help on stderr, as before T017 |

The T014 fixture no longer adds its own `--output`; all 10 T014 tests pass
against the real global argument.

Checks that the tests are not vacuous:
- Red run against a stub (an empty `diagnostic()`, no `--output` in `Cli`):
  2 of 12 unit tests and 2 of 13 process tests failed.
  `json_intent_never_runs_a_legacy_command` and
  `bare_invocation_still_prints_help_on_stderr` passed, because the old binary
  rejected the unknown `--output` and already printed help; they guard
  behaviour that the new parser could break.
- Mutation (reverted): binding a `--output json` invocation and running it as
  human made `json_intent_never_runs_a_legacy_command` fail, so the test does
  catch a JSON request reaching a legacy command.
- 5 consecutive runs of `order_list_process`: 5/5 pass.

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions output_intent` | PASS: 12 tests |
| `cargo test --locked --test order_list_process` | PASS: 13 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 250 tests (244 + 6), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Production lines: `src/cli.rs` 89, `src/cli/args.rs` 80,
`src/cli/invocation.rs` 257.

Limitations:
- Human-mode argument errors lose Clap's detailed text (for example the name
  of the unknown flag and "did you mean" hints), because the contract forbids
  raw argument values in diagnostics.
- JSON error envelopes are not printed yet (T018).
- `order list` and the `--mostro-pubkey` rename are not implemented; `order list`
  is still a test fixture grammar.

T001-T017 are complete. T018 (registry, capability guard and output boundary)
is next. The JSON pilot is still unimplemented.

## T018 Registry, JSON Guard And Output Boundary: 2026-10-22

Changed `src/cli/commands.rs`, `src/cli.rs`, `src/main.rs`, `src/outcome.rs`;
new `src/parse_machine.rs`. Native Linux, rustc 1.97.0, base `298f2cc`, code
commit `a68b697`.

Behaviour:
- `REGISTRY` lists all 33 subcommands with the canonical names approved on
  2026-10-22 (recorded in
  [the CLI contract](../specs/001-order-list-json/contracts/cli.md#canonical-command-names)),
  all `JsonSupport::Unsupported`. `lookup` resolves a one-word command path.
- With `--output json`, before RPC routing, environment access or
  `init_context`:
  - a valid legacy invocation prints an `output_not_supported` envelope;
  - an argument error prints an `invalid_arguments` envelope;
  - root options with no command (`mostro-cli --output json`) print
    `invalid_arguments` with command null.

  All exit 2, with the canonical name or null and stages
  `not_attempted`/`unknown`/`not_attempted`.
- `parse_machine::emit` serializes the whole outcome before writing one line.
  An unserializable outcome is replaced by an `internal_error` envelope for the
  same command and stages (exit 1). A failed write exits 1 with a fixed stderr
  line and appends nothing.
- `run()` returns the exit status and `main` calls `process::exit` once; the
  library no longer exits from inside. Help/version use `clap::Error::print`
  and its exit code.

| Test | Proves |
| --- | --- |
| `registry_matches_the_approved_mapping` | The 33 (CLI name, canonical name) pairs equal the approved table, in order |
| `every_subcommand_is_registered_exactly_once` | Registry and Clap subcommands are the same set; canonical names are unique |
| `canonical_names_are_dotted_lowercase` | `noun.verb` with lowercase words and kebab-case verbs |
| `no_command_supports_json_yet` | Every entry is human-only with not-executed stages |
| `lookup_resolves_only_registered_leaves` | Known one-word paths resolve; empty, unknown, `help` and `order list` do not |
| `writes_one_json_line_and_returns_the_outcome_exit` (unit) | Exactly one newline-terminated document; exit 0 or 2 from the outcome |
| `unserializable_outcome_becomes_one_internal_error` (unit) | One `internal_error` document for the same command and stages, exit 1, no serializer text |
| `failed_write_exits_1_and_appends_nothing` (unit) | A pipe closing after 10 bytes leaves only those bytes; fixed stderr line; exit 1 |
| `json_intent_gets_one_error_envelope_before_any_effect` (process) | Six JSON invocations, including `getlasttradeprivkey` and an unknown command: exit 2, one exact envelope, empty stderr, no sentinel, no files, zero relay connections |

The process test replaces T017's `json_intent_never_runs_a_legacy_command`,
which checked only exit 2 and absence of effects.

Checks that the tests are not vacuous:
- Red run against stubs (empty registry, `lookup` returning `None`, `emit`
  writing nothing): 7 of 9 new tests failed. `canonical_names_are_dotted_lowercase`
  and `no_command_supports_json_yet` passed over the empty registry and check
  real entries now.
- Two temporary mutations, each reverted: removing the `internal_error`
  fallback failed 1 unit test; making `lookup` never resolve failed the
  process test.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib parse_machine` | PASS: 3 tests |
| `cargo test --locked --test cli_functions` | PASS: 61 tests |
| `cargo test --locked --test order_list_process` | PASS: 13 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 258 tests (250 + 8), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Production lines: `src/cli.rs` 118, `src/cli/commands.rs` 384,
`src/parse_machine.rs` 50, `src/outcome.rs` 274, `src/main.rs` 14.

Limitations:
- `src/cli/commands.rs` is close to the 400-line limit; T030 adds `order list`
  there and will likely need to move the registry into its own module.
- Configuration and read errors are not emitted as JSON yet; no command runs in
  JSON mode until T030.
- Human-mode runtime errors still print `main`'s `eprintln!` text, unchanged
  legacy behaviour.

T001-T018 are complete. T019 (foundation gate) is next. The JSON pilot is still
unimplemented.

## T019 Foundation Gate: 2026-10-22

Native Linux, rustc 1.97.0. Base `778063f`; test commit `05bc385`. No
production code changed.

Before closing the gate, every foundation row of the
[quickstart acceptance matrix](../specs/001-order-list-json/quickstart.md#acceptance-matrix)
was matched to a test. One row was only partially proven: "registered legacy
JSON, including RPC and secret-export paths" had a secret-export case
(`getlasttradeprivkey`) but no daemon-RPC case. Commit `05bc385` closes it:
- Rejection runs now also point `MOSTRO_RPC_URL` at the loopback listener and
  set `MOSTRO_RPC_TOKEN` to the sentinel.
- `json_intent_gets_one_error_envelope_before_any_effect` gained
  `admmaintenancestatus` and `admcancelpending` rows (now 8 rows).
- The new positive control `human_rpc_command_reaches_the_configured_daemon`
  shows the same configuration does connect in human mode, so zero connections
  in JSON mode is evidence rather than an unused address.

The `admcancelpending` row first used `--order-id` and got `invalid_arguments`
instead of `output_not_supported`. The real flag is `--orderid` (`rename_all =
"lower"` also renames arguments), so the envelope was correct and the test was
fixed.

Proof obligations passed before user stories:

| Quickstart row | Evidence |
| --- | --- |
| Output flag at root/intermediate/leaf and equals form | `output_is_selected_at_every_depth_and_defaults_to_human`, `real_cli_declares_a_global_case_sensitive_output`; process rows with `--output=json` |
| Missing/invalid args with JSON intent, including unknown command names | `json_intent_gets_one_error_envelope_before_any_effect` (unknown flag, sentinel as command, root without command) |
| Values, hyphen values, short clusters, variadic values, `--` | `arities_and_hyphen_values_follow_clap`, `short_clusters_and_colliding_shorts_follow_scope`, `terminator_ends_option_parsing` |
| Invalid/missing/duplicate output | `duplicate_and_missing_declarations_are_invalid`, `argument_errors_exit_2_with_safe_stderr_before_any_effect` |
| Help/version with output json | `help_and_version_stay_text_on_stdout`, `bare_invocation_still_prints_help_on_stderr` |
| Registered legacy JSON, including RPC and secret-export | `json_intent_gets_one_error_envelope_before_any_effect`, `human_rpc_command_reaches_the_configured_daemon`, `no_command_supports_json_yet` |
| Planted secret sentinels in invalid args | Sentinel rows in both process tests (stdout and stderr) |
| Serialization and failing output writer | `unserializable_outcome_becomes_one_internal_error`, `failed_write_exits_1_and_appends_nothing` |
| Envelope shape, codes and examples | The 8 `outcome` unit tests, including the `examples.json` reproductions |
| SDK observability of EOSE, CLOSED, silence and disconnect | `convenience_fetch_reports_success_for_every_failure`, `explicit_subscription_distinguishes_completion_per_relay`, `disconnect_before_eose_is_observed_per_relay` |
| Harness isolation and watchdog | `child_reaches_the_loopback_relay_with_sandboxed_storage`, `watchdog_kills_and_reaps_a_hung_child` |

Not covered yet; these belong to user stories and nothing here claims them:
- configuration errors as JSON (T026, T030);
- listing success, empty, partial and failure outcomes, and the collector
  (US1, T020-T030);
- human/JSON field parity and verbose or identity-location streams (US2);
- removed `listorders`/`--mostropubkey` spellings (US1);
- offline schema, discovery and generated-schema drift (US3).

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions` | PASS: 61 tests |
| `cargo test --locked --test order_list_process` | PASS: 14 tests |
| `cargo test --locked --lib outcome` | PASS: 10 tests (8 envelope plus 2 matching the filter) |
| `cargo test --locked --lib parse_machine` | PASS: 3 tests |
| `order_list_process` 10 consecutive runs | PASS: 10 of 10, no leftover sandbox directories |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 259 tests (258 + 1), 0 failed |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

File-size finding: four legacy files still exceed 400 production lines,
counted with the T002 rule at the inventory commit `07f2595` and at HEAD:

| File | At `07f2595` | At HEAD |
| --- | ---: | ---: |
| `src/parser/dms.rs` | 1210 | 1210 |
| `src/db.rs` | 704 | 704 |
| `src/util/messaging.rs` | 600 | 639 |
| `src/cli/take_dispute.rs` | 403 | 403 |

- `util/messaging.rs` grew by 39 production lines in `0f2b644` (T007's
  `select_reply_event` extraction). That breaks the constitution's "do not grow
  legacy production sections" rule and was not reported in T007.
- The T002 table in `docs/architecture.md` lists 487, 695 and 1198 for
  `messaging.rs`, `db.rs` and `dms.rs`. Those numbers do not match the files at
  `07f2595`, which did not change `messaging.rs`.

The `messaging.rs` growth is resolved by `cdb1d35`
(`refactor: move reply correlation to util::reply`). It moves `is_mostro_reply`,
`ExpectedReply`, the message/event matchers, `select_reply_event` and their 8
unit tests into the new `src/util/reply.rs`:
- The moved production and test blocks are byte-identical to the originals
  (checked with `diff`).
- `wait_for_dm_matching` now builds its criteria with `ExpectedReply::new`.
- `crate::util` re-exports keep the public paths, so `tests/integration_tests.rs`
  is unchanged.
- Production lines: `util/messaging.rs` 556 (below its 600 baseline),
  `util/reply.rs` 90.
- Gates after the move: fmt, clippy, `cargo test --locked` (259 passed, 0
  failed), build and the 1.94.0 check all pass.

The inventory-table mismatch is still open. No file touched by T011-T019
exceeds the limit.

T001-T019 are complete. US1 starts at T020. The JSON pilot is still
unimplemented: no command runs in JSON mode yet.

## T020 Order-List Contract Tests: 2026-10-22

Native Linux, rustc 1.97.0. Base `7c1213f`. Commits `32ca095` (test),
`8323ddd` (fix), `613e69f` (contract tests and skeleton). Tracked in
[#209](https://github.com/MostroP2P/mostro-cli/issues/209).

Two decisions taken with the user on 2026-10-22:
- Test-first across tasks: a compile-only skeleton lands with the tests, and
  each new test is `#[ignore = "red until T0xx"]`. Every commit builds and the
  gates stay green; the implementing task removes its ignores.
- Event times above `i64::MAX` seconds saturate instead of wrapping (recorded
  in the data model and research §5).

### Timestamp regression

`parse_orders_events` converted the event time with `as i64`. A signed event
dated `u64::MAX` became `-1`, so an older `pending` state won the latest-state
reduction and the human table showed a 1969 date.
`event_times_beyond_i64_saturate_instead_of_wrapping` failed on `7c1213f`
(`left: Some(Pending)`, `right: Some(Canceled)`). The fix in
`src/parser/orders.rs` uses `i64::try_from(...).unwrap_or(i64::MAX)`, and the
test passes. The same `as i64` pattern in `parser/disputes.rs` and
`cli/get_dm_user.rs` is outside this feature and unchanged.

### Contract tests

`mod list_contract` in `tests/parser_orders.rs` targets two seams:
`outcome::order::{ListOrdersInput, PublicOrder, ListData}`, with
`ListData::from_selected` for already-selected rows, and
`commands::order::list::select_orders` for events to `ListData`. The DTOs use
core `Kind` and `Status`, so there is no second status enum.

| Ignored until | Cases | Covers |
| --- | ---: | --- |
| T022 | 26 | Default pending filters; case-insensitive side/status without trimming; `invalid_arguments` for untrimmed, empty, unknown and `all`; currency uppercased, not trimmed, empty kept; all 17 statuses through an exhaustive `match` on core `Status`; lowercase node hex; canonical UUID; empty `orders`; reducer order kept; `internal_error` for missing id/side/status/time or a row not matching the filters |
| T023 | 10 | Decimal strings at `i64::MIN`/`i64::MAX`/negative/zero; zero sats kept; nullable and unordered range endpoints with the fixed amount retained |
| T024 | 12 | Fiat code and payment text as received, including empty and control characters; RFC3339 `Z` times and null outside years 0000-9999 (both boundaries, `i64::MIN`/`i64::MAX`); one aggregate `invalid_event_timestamp` warning; `examples.json` `large_amount_success` and `empty_success` data reproduced exactly |
| T028 | 10 | Latest state before filtering; same-order ties keep the lowest event id; cross-order ties unordered; absent status matches no status filter; invalid status is pending; malformed events skipped; simple-form uppercase UUID made canonical; `u64::MAX` events saturate with one warning; normalized currency filter |

`examples.json` is a proposed contract; reproducing its data shows the DTOs
match it, not that a runtime produced it.

Checks that the tests are not vacuous:
- Red: `cargo test --test parser_orders -- --ignored` fails 58 of 58 against
  the stubs.
- Achievable: a throwaway prototype of `parse`, `from_selected` and
  `select_orders` (delegating to `parse_orders_events`) passed 58 of 58, so
  the expectations are consistent.
- Nine prototype mutations were all caught: trimming side/status, empty
  currency to null, no four-digit year bound, `+00:00` instead of `Z`, no
  invariant check, no timestamp warning, dropped range minimum, uppercase node
  hex, and selection without the status filter.
- The stubs were restored from copies and checked with `cmp`; both files
  still contain only `todo!()` bodies.

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders` | PASS: 39 passed, 58 ignored |
| `cargo test --locked --test parser_orders -- --ignored` | FAIL as intended: 0 of 58 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 260 passed (259 + 1), 0 failed, 58 ignored |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Production lines: `src/outcome/order.rs` 86, `src/commands/order/list.rs` 19,
`src/outcome.rs` 276, `src/parser/orders.rs` 296.

T001-T020 are complete. T021 (collector contract tests) is next; T022 needs
both T020 and T021. The JSON pilot is still unimplemented.

## T021 Collector Contract Tests: 2026-10-23

Native Windows (`x86_64-pc-windows-msvc`), rustc 1.97.0, base `e8711d1`. This
is the first native Windows run since T005; Linux was not rerun. Nothing is
committed.

Before T021, the Spec Kit artifacts were reconciled with a cross-artifact
analysis. The fixes:
- T021 moves to `tests/order_list_process.rs`, which already holds the relay
  harness.
- `msg` becomes a narrow public module so the tests can call `fetch_msg`.
- `ReadLimits` lets tests inject the time budgets, the event cap and the
  notification capacity.
- The canonical-name table in the contract no longer claims the roadmap lacks
  dotted names; the roadmap now links to that table.
- The task counts, the plan's dependency list, the quickstart and the checklist
  were brought up to date.

New compile-only stubs, following the T020 convention:

| File | Stub | Implemented by |
| --- | --- | --- |
| `src/msg/mod.rs` | `PublicReadConfig::new` (`todo!`); `ReadLimits` with its production `Default` | T025 |
| `src/msg/receive_msg.rs` | `fetch_msg`, `Fetched`, `FetchFailure` | T026 |
| `src/outcome/order.rs` | `Snapshot`, `CompletionPolicy`; RFC3339 serializer is `todo!` | T026 |

Eleven tests in `tests/order_list_process.rs`, each
`#[ignore = "red until T026"]`. All use generated keys, loopback relays and a
fixed query clock (2026-10-16T00:00:00Z):

| Test | Proves |
| --- | --- |
| `collector_sends_the_requested_filter_once_and_returns_its_events` | One REQ per relay, carrying exactly the author/kind/z/since/limit filter; events and a complete snapshot returned |
| `events_failing_verification_or_the_filter_are_dropped` | A forged event, another author, another kind, another `z` and an event before `since`, served by a relay that ignores the filter, are all excluded |
| `duplicate_relays_and_events_count_once` | Trimmed, empty and duplicate relay entries make 2 relays with one connection each; a shared event counts once |
| `a_closed_relay_fails_the_whole_read` | CLOSED is `relay_read_failed`; no events returned |
| `a_relay_disconnecting_before_eose_fails_the_read` | Disconnect before EOSE is `relay_read_failed` |
| `a_silent_relay_times_out_and_cleanup_stays_bounded` | `read_timeout` with counts 2/1/1 (the `partial_relay_timeout` example); elapsed time bounded by read plus cleanup; the answering relay's connection closes |
| `an_unreachable_relay_fails_before_the_query` | A listener that never accepts gives `relay_connection_failed` with a null snapshot, within the connect budget |
| `events_after_a_relays_eose_are_ignored` | An event delivered to the first relay's live subscription after its EOSE is not returned, while a second relay is still pending |
| `a_relay_filling_its_notification_buffer_fails_the_read` | 20 events succeed by default; with capacity 8 the read fails as `relay_read_failed` with no completed relay |
| `exceeding_the_event_cap_fails_without_truncating` | Cap 4 succeeds with 4 events; cap 3 gives `read_limit_exceeded` with `received_events` 3 |
| `cancelling_a_read_closes_its_connections` | Dropping the read while a relay holds its REQ closes the other relay's connection |

The harness gained per-listener open-session counting (`wait_idle`) and three
query policies: `RecordQueries`, `IgnoreFilter` and `Gate`. A new dev
dependency, `nostr-memory` 0.45.2, provides the store for the forged-event
relay. It was already locked through `local-relay`; `Cargo.lock` gains only
its entry in mostro-cli's dependency list. SDK findings that shape T026 are in
[research §2](../specs/001-order-list-json/research.md#2-empty-results-and-completion).

Checks that the tests are not vacuous:
- Red run against the stubs: `cargo test --locked --test order_list_process
  -- --ignored` fails 11 of 11.
- Achievable: a throwaway prototype of `PublicReadConfig::new`, `fetch_msg`
  and the snapshot serializer passed 11 of 11. It also passed 10 consecutive
  runs, leaving no `mcli-process-*` directories.
- Six prototype mutations, each reverted, were caught by the tests listed:
  - Not freezing a relay at its EOSE: the late-event test.
  - Skipping the local filter match: the verification/filter test.
  - Removing the notification-capacity rule: the buffer test.
  - Leaking the client on drop: the cancellation and unreachable-relay tests.
  - Not deduplicating relay URLs: the duplicate test.
  - Returning partial success on timeout: the silent-relay test.
- A seventh mutation, dropping the explicit shutdown-on-drop guard, was not
  caught. Dropping the last `Client` handle already closes its connections,
  so the guard is not required.
- The stubs were restored from saved copies and their hashes compared. The
  prototype is not in the tree.

Limitations:
- Waiting for the late event uses a 300 ms pause. A shorter pause can only let
  a broken collector pass, never fail a correct one. The EOSE-freeze mutation
  was caught with the pause.
- The `cleanup_incomplete` warning after a successful read is not exercised:
  no fixture can stall the SDK's unsubscribe and shutdown.
- The silent-relay and unreachable-relay tests assert that the answering
  relay's connection closes. They do not observe the silent endpoint, which
  never notices its peer leave.
- `watchdog_kills_and_reaps_a_hung_child` failed once, after 7 s, while another
  cargo build ran concurrently. It passed in the clean full run below; run
  cargo jobs one at a time on loaded Windows hosts.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process` | PASS: 14 passed, 11 ignored |
| `cargo test --locked --test order_list_process -- --ignored` | FAIL as intended: 0 of 11 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 256 passed, 0 failed, 69 ignored (58 from T020, 11 from T021) |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

The Windows total (256) is below Linux's 260 at T020 because Windows skips
`non_unicode_values_keep_json_intent` (Unix-only) and runs different
platform-specific cases. This is the native Windows result for the current
tree; T051-T052 still own the full platform acceptance.

Production lines: `src/msg/mod.rs` 67, `src/msg/receive_msg.rs` 67,
`src/outcome/order.rs` 116.

T001-T021 are complete. T022 (ListOrdersInput/ListData) is next. The JSON
pilot is still unimplemented.

## T022-T024 Order-List DTO Mapping: 2026-10-23

Native Windows, rustc 1.97.0, base `e8711d1` plus the uncommitted T021 work.
Linux was not rerun.

T022's tests call `ListData::from_selected`, which serializes whole rows, so
the money fields (T023) and the text and time fields (T024) cannot stay
`todo!` while T022 passes. The user chose to implement the three tasks as one
increment, removing each task's ignores and recording them separately here.

Changed only `src/outcome/order.rs` (185 production lines):
- `ListOrdersInput::parse` reuses core `Kind::from_str` and `Status::from_str`.
  Both are case-insensitive and untrimmed; no second status enum. Status
  defaults to pending, and an unknown, padded, empty or `all` value is
  `invalid_arguments`. Currency is uppercased only.
- `ListData::from_selected` keeps the reducer order and maps each row's
  explicit public fields:
  - Node key as lowercase hex.
  - The UUID's canonical lowercase hyphenated form.
  - Core side and status.
  - i64 amounts, range endpoints and premium through `to_string`: canonical,
    lossless, with no float.
  - Fiat code and payment method as received.
  - `created_at_unix` as the decimal event time.
  - `created_at` as UTC RFC3339 with `Z`, or null outside years 0000-9999.
- A row missing id, side, status or event time, or not matching the filters,
  is `internal_error`. Out-of-range times add one aggregate
  `invalid_event_timestamp` warning.

| Ignored until | Cases | Now |
| --- | ---: | --- |
| T022 | 26 | pass, ignores removed |
| T023 | 10 | pass, ignores removed |
| T024 | 12 | pass, ignores removed; `large_amount_success` and `empty_success` data reproduced exactly |
| T028 | 8 | still ignored; fail at the `select_orders` stub |

The run counted 50 T022-T024 cases and 8 T028 cases. T020's table (48 and 10)
put two cases in the wrong group; the 58-case total is unchanged. The red
evidence is T020's run against the stubs.

Mutation checks on the real code, each reverted:
- Skipping the filter-invariant check failed 3 internal-error cases.
- Widening the year bound to 99999 failed the year-10000 case.

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders list_contract -- --ignored` (before removing ignores) | 50 passed; 8 T028 cases fail at the stub |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | **FAIL**, see the next note. With `--no-fail-fast`: 305 passed, 1 failed, 19 ignored (11 T021, 8 T028); `parser_orders` 89 passed, 8 ignored |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

**Windows failure outside this change**: `watchdog_kills_and_reaps_a_hung_child`
failed in two consecutive full runs, taking about 7 s against a bound of 2 s
plus 5 s. Run alone, it passed 3 of 3 times in about 2.1 s. Its time bound
includes spawning the child, and the target spawns many `mostro-cli`
children in parallel. On this Windows host that contention, possibly with
on-access scanning of the freshly linked binary, pushes the spawn past the
bound. The T021 full run passed it. No code it exercises changed in this
increment. It stays an open Windows gate failure, not fixed here.

T001-T024 are complete. T025 (public configuration and private Session) is
next. The JSON pilot is still unimplemented: no command uses these types yet.

## T025 Public Configuration And Session: 2026-10-23

Native Windows, rustc 1.97.0; uncommitted on top of T021-T024. Linux was not
rerun.

Changed `src/msg/mod.rs` (166 production lines) and `src/util/net.rs`:
- `PublicReadConfig::resolve(cli_pubkey, cli_relays, env)` gives CLI values
  precedence and asks `env` only for values the CLI did not supply. An empty or
  invalid CLI value never falls back, and nothing calls `set_var`.
- Missing or blank values are `configuration_missing`; an unparsable key or
  relay URL is `configuration_invalid`. Keys use the existing SDK parser, so
  hex and npub are both accepted. `env` is a parameter, so tests need no
  process environment and production passes a reader of the real one.
- `PublicReadConfig::new` replaces the T021 stub: it trims entries, drops empty
  ones, validates `ws`/`wss` URLs and deduplicates by `RelayUrl` equality,
  which compares normalized URLs.
- Two helpers are now shared with the legacy `connect_nostr`, which uses them
  instead of its own copies: `relay_entries` (the comma split) and
  `ephemeral_client` (rustls provider plus a NIP-42 authenticator with fresh
  in-memory keys).
- `Session` is private to `msg`, with no client accessor. `Session::connect`
  adds every relay without automatic reconnection, applying the connect
  budget and notification capacity from `ReadLimits`. Any unconnected relay is
  `relay_connection_failed`. Its first production caller is `fetch_msg` in
  T026, so until then it carries
  `#[cfg_attr(not(test), allow(dead_code, reason = ...))]`, which T026 must
  remove. `expect` was tried first, but rustc 1.94 reported the expectation
  as unfulfilled where 1.97 did not.
- No database, identity, admin credential, `MCLI_DIR` or DM protocol probe is
  touched.

| Test (`msg::tests`) | Proves |
| --- | --- |
| `cli_values_win_and_environment_fills_the_rest` | CLI key with env relays; with both CLI values the environment is never consulted, and an invalid env key is not read |
| `empty_or_invalid_cli_values_do_not_fall_back` | Empty, blank or invalid CLI key or relays, `https://` and unparsable URLs give missing or invalid despite valid env values |
| `missing_values_are_configuration_missing` | Either or both absent |
| `npub_is_accepted_and_kept_as_the_same_key` | npub input yields the same key |
| `relays_are_trimmed_and_deduplicated_after_normalization` | `wss://Relay.Example:443` and `wss://relay.example/` count once; blank entries dropped; order kept |
| `only_public_variables_are_read` | With `ADMIN_NSEC`/`MOSTRO_RPC_TOKEN` sentinels present, only `MOSTRO_PUBKEY` and `RELAYS` are requested; the config's Debug output has no sentinel |
| `an_unanswered_relay_fails_the_session_within_the_budget` | A listener that never accepts gives `relay_connection_failed` within the connect budget |

Tests and code landed together. Two mutations, each reverted, failed 2 of the
7 tests:
- Letting an empty CLI key fall back to the environment.
- Dropping relay deduplication.

T021's collector tests now get past `PublicReadConfig::new` and stop at the
`fetch_msg` stub, so they stay ignored until T026.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib msg` | PASS: 7 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | 312 passed, 1 failed (`watchdog_kills_and_reaps_a_hung_child`), 19 ignored |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS, no warnings after the `allow` change |
| `cargo test --locked --test order_list_process` three times in a row | Run 1 FAIL (watchdog, 10.3 s); runs 2 and 3 PASS (14 passed, about 2.7 s) |

The watchdog failure repeats the T022-T024 note and narrows its cause. It
fails only on the first process run after `mostro-cli.exe` is relinked, and
then the whole target takes about 10 s instead of 2.7 s. That points to
slow first spawns of a fresh binary on this Windows host, not to this change.
It remains open.

T001-T025 are complete. T026 (`fetch_msg` and the snapshot) is next. The JSON
pilot is still unimplemented.

## T026 Collector: 2026-10-23

Native Windows, rustc 1.97.0; uncommitted on top of T021-T025. Linux was not
rerun.

`fetch_msg` in `src/msg/receive_msg.rs` (249 production lines) replaces the
T021 stub, following the constraints in
[research §2](../specs/001-order-list-json/research.md#2-empty-results-and-completion):
- A filter without `since` or `limit` is `internal_error` before any
  connection, so every read is bounded.
- It opens a `Session` (T025), which connects every relay within the connect
  budget. A failure there is `relay_connection_failed` with a null snapshot.
- It arms each relay's notification stream before sending one REQ with a
  generated subscription id, and observes the streams merged until every relay
  reaches EOSE or the read deadline passes. The deadline starts before the REQ.
- Per relay:
  - EVENT frames for this subscription are kept only if they match the filter;
    the SDK has already verified them.
  - Duplicates across relays count once.
  - EOSE freezes the relay, and later frames from it are ignored.
  - CLOSED or a disconnect, terminate or shutdown status is `relay_read_failed`.
  - Reaching `notification_capacity` notifications before EOSE is
    `relay_read_failed`, because the SDK drops lagged notifications silently.
- More than `max_unique_events` unique events is `read_limit_exceeded`. A
  deadline is `read_timeout`, and all streams ending is `relay_read_failed`.
  Nothing is retried, and a failure returns no events.
- Cleanup unsubscribes and shuts the client down within the cleanup budget. If
  it times out or the unsubscribe fails, a `cleanup_incomplete` warning is
  added without replacing an original error.
- `Snapshot::new` and the RFC3339 serializer replace their stubs. The
  serializer reuses T024's four-digit-year helper; an unrepresentable time
  fails serialization (so the output boundary emits `internal_error`) rather
  than writing a wrong date.
- `Session`'s temporary `allow(dead_code)` from T025 is removed.

The 11 T021 tests pass and their ignores are removed. Two mutations, each
reverted, failed the expected tests:
- Not freezing a relay at EOSE failed the late-event test.
- Skipping the local filter match failed the verification/filter test.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process -- --ignored` (before removing ignores) | PASS: 11 of 11 |
| `cargo fmt --all -- --check` | PASS after applying rustfmt's layout |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | 323 passed, 1 failed (`watchdog_kills_and_reaps_a_hung_child`), 8 ignored (T028) |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |
| `cargo test --locked --test order_list_process` twice | Run 1 FAIL (watchdog, 10.3 s); run 2 PASS: 25 of 25 in 3.0 s |

The watchdog failure follows the same first-run-after-relink pattern recorded
for T025. It does not exercise `fetch_msg`.

T001-T026 are complete. T027 (moving the order reducer into `msg::orders`) is
next. The JSON pilot is still unimplemented: no command calls `fetch_msg`
yet.

## T027 Order Reducer Extraction: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T021-T026. Linux was not
rerun.

The private `src/msg/orders.rs` (84 production lines) now owns the order
query, decoder and reducer:
- `seven_day_filter` holds the seven-day author/kind/`z`/limit-50 filter that
  `util::events::create_seven_days_filter_at` built; that helper now delegates
  to it and keeps its error text for the order and dispute callers.
- `reduce_orders` takes the SDK's `BTreeSet<Event>` order (timestamp
  descending, event id ascending), keeps the first decodable event per order
  id as its latest state, and filters by status, currency and side only after
  that reduction. Rows come out newest first without a second sort. Tag
  decoding is unchanged (`nip33::order_from_tags`); events without an id or
  side are skipped, and times beyond `i64` saturate.
- `parser::orders::parse_orders_events` delegates to it, which drops about 55
  production lines from `src/parser/orders.rs`. No CLI aliases were added.
- `commands::order::list::select_orders` stays a `todo!("T028")` stub. T028
  must build it on `reduce_orders` instead of adding a second reducer.

The module is declared `mod orders;` and re-exported `pub(crate)`, so it is
not part of the library API.

One mutation, reverted afterwards: making the per-id dedup a no-op failed 5
parser tests (latest-before-status/currency/side filters, same-time ties,
saturated far-future times).

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders` (T020 slice) | PASS: 89 passed, 8 ignored (T028) |
| `cargo test --locked --test integration_tests` | PASS: 14 of 14 |
| `cargo test --locked --test order_list_process` (T021 slice) twice | Run 1 FAIL (watchdog, 10.9 s); run 2 PASS: 25 of 25 in 2.9 s |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked` | All suites pass except `watchdog_kills_and_reaps_a_hung_child` on the first run after relinking |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

The watchdog failure repeats the first-run-after-relink pattern recorded for
T025 and T026. It does not touch the reducer.

T001-T027 are complete. T028 (`order.list` command on `fetch_msg` and
`reduce_orders`) is next. No command calls `fetch_msg` yet.

## T028 Order-List Operation: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T021-T027. Linux was not
rerun.

`src/commands/order/list.rs` (under 90 production lines) implements the typed
operation; `commands/mod.rs`, `commands/order/mod.rs` and `lib.rs` already
declared it:
- `list_orders(side, currency, status, config, limits, now)` returns one
  `Envelope<ListData, Snapshot>` for `order.list` and never prints, exits,
  opens the database or touches a Nostr client.
- Filters are parsed by `ListOrdersInput::parse` before the `config` closure
  runs, so invalid arguments never read node configuration. Argument,
  configuration and pre-query errors carry a null snapshot.
- Its only network access is one `fetch_msg` with the seven-day order filter
  at the injected clock. A failed read keeps its code, warnings and snapshot
  with null data.
- `select_orders` builds on `msg::reduce_orders` (no second reducer) and
  `ListData::from_selected`. It now borrows the event set, so the parser test
  helper passes `&`.
- Read errors and successes use `ExecutionMeta::read`: every write stage is
  `not_applicable`.
- `Warnings::merge` in `src/outcome.rs` combines collector and mapping
  warnings, still one per code.

CLI wiring, the human renderer and binary-level cases remain T029-T031.

The 8 T020 selection tests are no longer ignored and pass. Three new tests in
`tests/order_list_process.rs` call `list_orders` against loopback relays:
latest-state selection across two relays with snapshot counts, filter
validation before configuration (the config closure panics if called) plus
`configuration_missing`, and a CLOSED relay failing the read while the
snapshot is kept.

One mutation, reverted afterwards: dropping the status filter in
`select_orders` failed 3 selection tests and the cross-relay command test
(it surfaced as `internal_error` from the mapping invariant).

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders` | PASS: 97 of 97, 0 ignored |
| `cargo test --locked --test order_list_process listing` | PASS: 3 of 3 |
| `cargo fmt --all -- --check` | PASS after applying rustfmt's layout |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | All suites pass except `watchdog_kills_and_reaps_a_hung_child` on the first run after relinking |
| `cargo test --locked --test order_list_process` second run | PASS: 28 of 28 in 3.1 s |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

The watchdog failure repeats the first-run-after-relink pattern recorded for
T025-T027.

T001-T028 are complete. T029 (typed list rendering in `parse_machine` and
`parse_human`) is next. No CLI command reaches `list_orders` yet.

## T029 Typed List Rendering: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T021-T028. Linux was not
rerun.

Both output branches can now render a `ListOutcome`:
- `parse_machine::emit` was already generic over the envelope. Two new unit
  tests drive it with a real `ListOutcome`: one success line carrying
  `data.orders`, `data.filters` and `meta.snapshot`, and a snapshot time
  outside four-digit years turning into one `internal_error` document with
  null data and snapshot.
- New `src/parse_human/table.rs` holds the order table (headings, market for
  zero sats, `min-max`/`min-?`/`?-max`, UTC-minute or Invalid date, colors,
  fixed-width id column, "No Offers" wording) built from an `OrderRow` of
  display strings.
- `parser::orders::print_orders_table` now maps `SmallOrder` to `OrderRow`
  and delegates, keeping its legacy defaults (absent status shown as active,
  absent time as 1970). `print_order_preview` reuses the shared range helper.
  `src/parser/orders.rs` lost about 120 production lines.
- New `src/parse_human/order.rs` `render_list(&ListData)` prints the filter
  summary, node key and the same table from typed data only: no network,
  storage, logging or writes.
- `PublicOrder` and `ListData` fields became `pub(crate)` so the renderer
  reads them; the public API and JSON shape are unchanged.

A unit test renders one order through both paths and checks that the typed
view ends with exactly the legacy table, plus filter summary, node key, market,
`10-?` range and UTC minute. A second covers the empty-book wording and an
unrepresentable time showing Invalid date. Full presentation parity remains
T034-T036.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib parse_` | PASS: 10 of 10 |
| `cargo test --locked --test parser_orders` | PASS: 97 of 97 (legacy table tests unchanged) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | All suites pass except `watchdog_kills_and_reaps_a_hung_child` on the first run after relinking |
| `cargo test --locked --test order_list_process` second run | PASS: 28 of 28 in 3.0 s |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T029 are complete. T030 (CLI wiring of `order list` and the root
`--mostro-pubkey` spelling) is next.

## T030 Order-List CLI Wiring: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T027-T029 (pushed as
`b8ce0e4`/`314262b`). Linux was not rerun.

- The command registry moved from `src/cli/commands.rs` to
  `src/cli/registry.rs`. Entries now carry a leaf `path`, so `order list`
  registers as `["order", "list"]` with canonical `order.list` and
  `JsonSupport::Read`; every other entry is unchanged and human-only.
- `listorders` and its `--kind` spelling are gone, replaced by
  `order list [--side buy|sell] [--currency TEXT] [--status STATUS]`. Side and
  status use Clap possible values (case-insensitive, untrimmed) from the new
  shared `outcome::order::{SIDES, STATUSES}`, which a parser test pins to the
  data-model status set. The legacy `src/cli/list_orders.rs` handler was
  deleted; its status tests in `tests/integration_tests.rs` now use
  `ListOrdersInput::parse`.
- The root `--mostropubkey` spelling became `--mostro-pubkey` (`-m` kept).
  No aliases; other legacy spellings are unchanged.
- `cli::run` binds the arguments once and routes `order list` in both modes
  to `list_orders` before RPC routing, environment mutation, logging or
  `Context`. Configuration comes from `PublicReadConfig::resolve` over the
  CLI values and `MOSTRO_PUBKEY`/`RELAYS` without `set_var`. JSON goes
  through `parse_machine::emit`; human prints `render_list` with one stdout
  write, or one fixed `error: <message>` line on stderr with the outcome's
  exit code.
- JSON rejection of other commands now resolves the full subcommand path.
  Argument errors resolved to `order.list` use read stages
  (`not_applicable`); legacy and unresolved rejections keep
  `not_attempted`/`unknown`/`not_attempted`.
- `Envelope` gained `data()` and `error_code()` accessors for the human path.

Tests:
- `tests/cli_functions.rs`: the decision-table grammar tests now run against
  the real `Cli` instead of a test-built `order list` grammar. New coverage
  for every side/status spelling, and for removed spellings (`listorders`,
  `--kind`, `--mostropubkey`) failing as argument errors, including
  `--output json listorders` as `invalid_arguments` with command null.
  Registry tests compare leaf paths, and `only_order_list_supports_json`
  replaces `no_command_supports_json_yet`.
- `tests/order_list_process.rs`: binaries now run `order list`. Missing
  configuration gives exactly `error: Required node configuration is
  missing.` with exit 1 and no files written; a configured empty relay
  prints the human table with exit 0 and leaves HOME, MCLI_DIR and cwd
  empty. JSON rejection rows use `listdisputes`. New
  `order_list_json_uses_read_stages` checks `--side both` and `--kind` as
  JSON `invalid_arguments` for `order.list` with read stages and no
  connection, plus one JSON success over the loopback relay.

One mutation, reverted afterwards: marking `order.list` as
`JsonSupport::Unsupported` failed `only_order_list_supports_json` and
`order_list_json_uses_read_stages` (argument errors reverted to
`not_attempted` stages).

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions --test integration_tests --test parser_orders --test order_list_process` | PASS: 61, 14, 98 and 29 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | All suites pass (lib 97, cli_functions 61, integration 14, disputes 9, dms 19, parser_orders 98, util 15) except `watchdog_kills_and_reaps_a_hung_child` on the first run after relinking |
| `cargo test --locked --test order_list_process` second run | PASS: 29 of 29 in 4.1 s |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Documentation (`docs/commands.md`, `README.md`) still shows `listorders` and
`--mostropubkey`; T049 owns that migration text.

T001-T030 are complete. T031 (binary success/failure/identity matrix for
`order list`) is next.

## T031 Order-List Binary Matrix: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T030 (pushed as
`cce8203`/`5e301fc`). Linux was not rerun. Test-only change in
`tests/order_list_process.rs`; no production code changed.

Every case runs the real `mostro-cli` child against loopback relays on the
real clock, with the existing sandbox (allow-listed environment, isolated
HOME/USERPROFILE/MCLI_DIR/cwd, closed stdin, captured streams, 40-second
watchdog). Each run asserts the child exited and was reaped. JSON runs go
through `one_document`: exactly one newline-terminated line, parsed whole by
Serde (anything after the document fails), empty stderr, `status` agreeing
with the exit code, schema version 1 and read stages `not_applicable`.

| Test | Cases |
| --- | --- |
| `order_list_binary_selects_rows_in_both_modes` | JSON success: pending rows 1 and 2 newest first, order 3 hidden by its later canceled state, node key, no warnings, snapshot counts; human `--side sell` lists only order 1; the relay stores exactly the same events afterwards (no EVENT published); fresh HOME, MCLI_DIR and cwd stay empty |
| `order_list_binary_empty_results_are_successes` | Filtered-empty (`--currency eur` over a non-empty book) and EOSE-empty relay: exit 0, `orders: []`, received counts 3 and 0 |
| `order_list_binary_read_failures_return_no_rows` | CLOSED alone and answering+CLOSED (partial failure): exit 1, `relay_read_failed`, data null, snapshot kept with configured relay count; human mode prints exactly the fixed stderr line with empty stdout |
| `order_list_binary_times_out_on_a_silent_relay` | Silent relay at production budgets: exit 1 after at least the 15-second read budget, `read_timeout`, snapshot 1 configured / 0 completed / 0 received |
| `order_list_binary_ignores_existing_and_unusable_identity_locations` | An existing synthetic `mcli.db` is left byte-identical and alone; `MCLI_DIR` naming a regular file still lists successfully and leaves the file and HOME untouched |

Finding while writing the tests: kind 38383 is addressable, so `LocalRelay`
keeps only the newest state per `d` tag. The first run expected 4 received
events and got 3; the tests now take the relay's stored set as the baseline
instead of the seeded list. This is relay behavior, not a CLI defect.

Not covered here: NIP-42 AUTH. The loopback relays do not request it, so
"AUTH allowed" is not exercised; the no-EVENT check compares stored events
before and after each run. Exit 2 binary cases remain in
`order_list_json_uses_read_stages` and `argument_errors_exit_2_with_safe_stderr_before_any_effect`.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process order_list_binary` | PASS: 5 of 5 in 15.2 s (first run: 2 failed on the addressable-event count, fixed in the test) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 97, cli_functions 61, integration 14, order_list_process 34 (15.9 s), disputes 9, dms 19, parser_orders 98, util 15; the watchdog test also passed on this first run |
| `cargo test --locked --test order_list_process` second run | PASS: 34 of 34 in 16.0 s |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T031 are complete. T032 (allow-listed diagnostics and verbose behavior
on the migrated path) is next.

## T032 Order-List Diagnostics And Verbose: 2026-10-26

Native Windows, rustc 1.97.0; uncommitted on top of T031 (`220577f`/`d8df6a3`,
not yet pushed). Linux was not rerun.

Starting point, confirmed by the first run below: the `order list` path never
installs a `log` logger or a `tracing` subscriber, so `RUST_LOG=trace` and
`--verbose` produced no SDK output. `--verbose` simply did nothing there.

Changes:
- `src/cli.rs`: with `--verbose`, the public listing writes exactly one
  allow-listed stderr line after its outcome: `verbose: order.list <ok|code>;
  <received> events from <completed>/<configured> relays`, or `...; no read`
  when no query started. Only counts and the stable code appear; never relay,
  URL, argument, server or SDK text. Still no logger on this path.
- `src/outcome.rs`: `Envelope::snapshot()` accessor; `Snapshot` relay/event
  counts became `pub(crate)` for that line.
- `src/msg/orders.rs`: the decode-failure log is fixed text at debug level
  instead of the raw error.
- `src/main.rs` is unchanged: on this path its only `Err` is the fixed
  argument-binding message.

New test `order_list_diagnostics_are_allow_listed_in_verbose_mode` runs the
binary with `--verbose` and `RUST_LOG=trace`, in JSON and human mode, for:
success over a relay URL whose query carries a sentinel; CLOSED whose server
reason carries the sentinel; sentinel `MOSTRO_PUBKEY`, `--mostro-pubkey` and
`--relays`. For each it checks the exit code, no sentinel or loopback address
on stdout or stderr, every stderr line starting with `verbose: order.list ` or
`error: `, exactly one verbose line with the expected counts/code, one JSON
document in JSON mode, empty stdout on human errors and no sandbox effects.

Evidence:
- Before the change the test failed only on the missing verbose line
  (`left: 0, right: 1` for the first row); nothing leaked.
- After it the test passes (1 of 1, 17.2 s).
- Mutation, reverted afterwards: calling `pretty_env_logger::init()` on this
  path when verbose made the test fail with the sentinel URL in stderr
  (`DEBUG nostr_sdk::relay::inner > Connecting to 'ws://127.0.0.1:…/?token=secret-sentinel-7f3a'`
  and the raw WebSocket request).

Open, outside this task: legacy commands still initialize `pretty_env_logger`
under `--verbose` (`src/cli/context.rs`), which, as the mutation shows, lets
the SDK print relay URLs. They are not on the migrated path.

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process order_list_diagnostics` (before) | FAIL as expected: missing verbose line |
| same (after) | PASS: 1 of 1 |
| same with the logger mutation | FAIL: sentinel URL in stderr |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | All suites pass (lib 97, cli_functions 61, integration 14, disputes 9, dms 19, parser_orders 98, util 15) except `watchdog_kills_and_reaps_a_hung_child` on the first run after relinking (order_list_process 34 of 35) |
| `cargo test --locked --test order_list_process` second run | PASS: 35 of 35 in 15.9 s |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T032 are complete. T033 (US1 focused matrix and internal milestone) is
next.

## T033 US1 Milestone (Internal): 2026-10-26

Native Windows, rustc 1.97.0, on `develop` at `6c9a670` (T032; T031-T032
commits not yet pushed). Linux was not rerun. No code changed in this task.
This is the US1 internal milestone, not pilot or release acceptance: US2
presentation parity, US3 discovery/schemas and the polish gates remain.

### Coverage Map

| US1 obligation | Automated evidence |
| --- | --- |
| Selected fields and encoding | `tests/parser_orders.rs` `list_contract` (98 tests in the file): canonical ids, side, every status, signed i64 money as decimal strings, zero-market sats, nullable range endpoints, received text as-is, UTC RFC3339 or null with one aggregated warning, proposed `examples.json` reproduced |
| Selection rules | `parser_orders`: latest state before filters, same-time ties keep the lowest event id, unspecified cross-order ties, absent status never matches, malformed events skipped; binary `order_list_binary_selects_rows_in_both_modes` |
| Empty versus error | Binary: filtered-empty and EOSE-empty are exit 0 with `orders: []`; CLOSED, partial-relay CLOSED and silent-relay timeout are exit 1 with data null and the snapshot kept (`order_list_binary_*`) |
| Exit codes and envelope | Exit 0/1/2 agree with status; one fully consumed JSON document; read stages `not_applicable`; argument errors keep `order.list` (`order_list_json_uses_read_stages`) |
| Limits | 7-day author/kind/`z` filter with per-relay limit 50 (`integration_tests` fixed-clock filter, snapshot fields); 10,000-event cap fails without truncation; notification capacity; production 15 s read budget (`order_list_binary_times_out_on_a_silent_relay`) |
| No effects | No EVENT published (relay store unchanged); fresh HOME/MCLI_DIR/cwd stay empty; existing `mcli.db` byte-identical; unusable `MCLI_DIR` still lists; no relay connection on argument errors |
| Diagnostics | Planted sentinels in relay URL, CLOSED reason and CLI/env config never reach stdout/stderr under `--verbose` and `RUST_LOG=trace`; exactly one allow-listed verbose line |

### Migrated File Sizes

Production lines before the first `#[cfg(test)]`, against the 400-line limit:

| File | Production lines |
| --- | --- |
| `src/cli.rs` | 217 |
| `src/cli/registry.rs` | 89 |
| `src/cli/commands.rs` | 313 |
| `src/commands/order/list.rs` | 83 |
| `src/msg/mod.rs` | 160 |
| `src/msg/orders.rs` | 82 |
| `src/msg/receive_msg.rs` | 249 |
| `src/outcome.rs` | 295 |
| `src/outcome/order.rs` | 239 |
| `src/parse_machine.rs` | 50 |
| `src/parse_human/order.rs` | 41 |
| `src/parse_human/table.rs` | 119 |
| `src/parser/orders.rs` | 122 |

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --no-fail-fast --test parser_orders --test integration_tests --test order_list_process` | parser_orders 98 of 98, integration_tests 14 of 14, order_list_process 34 of 35: `watchdog_kills_and_reaps_a_hung_child` failed its timing bound (two focused runs) |
| `cargo test --locked --test order_list_process` rerun | PASS: 35 of 35 in 15.9 s |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 97, cli_functions 61, integration 14, order_list_process 35, disputes 9, dms 19, parser_orders 98, util 15, doctests 0 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

### Open For Later Tasks

- NIP-42 AUTH is not exercised; the loopback relays never request it.
- Native Linux runs (T052) and CI wiring (T050-T051) are pending.
- `watchdog_kills_and_reaps_a_hung_child` still fails on some Windows runs
  (after relinking or under load); reruns pass. It needs a less timing-sensitive
  bound.
- Legacy commands still install `pretty_env_logger` under `--verbose`, which
  can print relay URLs (seen in the T032 mutation).
- Human mode does not show warnings; full human presentation is T034-T036.
- `README.md` and `docs/commands.md` still show `listorders` and
  `--mostropubkey` (T049).

T001-T033 are complete. US2 starts with T034 (renderer parity tests) and T035
(process parity tests).

## T034-T036 US2 Human Presentation: 2026-10-26

Native Windows, rustc 1.97.0, on `develop` after `98c6520`.

### T034 Renderer Parity (`tests/parser_orders.rs`, `list_contract`)

| Test | Evidence |
| --- | --- |
| `human_rows_carry_the_json_business_fields` | Six orders: every JSON row maps to one human row with the same id, side, status, sats (`0` as market, signed `-5`), currency, fixed or `min-max`/`min-?`/`?-max` fiat, payment, signed premium and UTC minute or Invalid date |
| `human_heading_and_filter_summary_follow_the_typed_filters` | Default shows only status pending and node hex; explicit filters show side, uppercased currency and status; all nine column headings |
| `human_empty_book_keeps_the_existing_wording` | "No Offers" wording with the filter summary and no order headings |
| `human_dates_are_the_utc_minute_or_invalid_date_like_json` | 8 cases: four-digit years show the UTC minute; years outside 0000-9999 and i64 extremes show Invalid date, exactly where JSON `created_at` is null |
| `human_text_escapes_terminal_controls` | ESC, BEL, CR, LF and C1 CSI in currency, payment and the currency filter render as Rust escapes; JSON keeps the raw text |

Red before T036: 4 failures, all expected. Year 10000 rendered as
`+10000-01-01 00:00`, year -1 as `-0001-12-31 23:59`, and the raw `\r` reached
the currency cell. They were ignored with a T036 reason until the fix.

### T035 Process Parity (`tests/order_list_process.rs`)

`order_list_human_and_json_describe_the_same_book` serves one book (two priced
EUR orders plus the live book) to fresh and existing (`mcli.db`) sandboxes. In each:
JSON, default human, `--output human`, `--verbose` and a closed-stdin pipe all
exit 0 with piped stdout. The four human tables are byte-identical. Verbose adds
only the allow-listed summary. Each JSON row's business fields appear in its
human row, and the row counts match. Neither sandbox is written, `mcli.db` stays
byte-identical and the relay store is unchanged. `run_with_watchdog` now closes
a piped stdin at spawn. This passed before T036 because the book has no control
text or out-of-range dates.

### T036 Implementation

- `src/parse_human/table.rs`: `escape_controls` shows every `char::is_control`
  character as `escape_default`; applied to currency and payment cells, so the
  legacy table also benefits.
- `src/parse_human/order.rs`: the currency filter line is escaped; the row
  passes no event time when the typed `created_at` is null, so Invalid date
  follows the outcome's single calendar rule.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders human_` before T036 | FAIL: 9 passed, 4 failed (expected red) |
| `cargo test --locked --test order_list_process order_list_human` | PASS: 1 of 1 in 1.5 s |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | All suites pass (lib 97, cli_functions 61, integration 14, disputes 9, dms 19, parser_orders 110, util 15) except `watchdog_kills_and_reaps_a_hung_child` (order_list_process 35 of 36) |
| `cargo test --locked --test order_list_process` reruns | FAIL once (watchdog), then PASS: 36 of 36 in 15.9 s |
| `cargo test --locked --test order_list_process watchdog` alone | PASS in 2.1 s |
| `cargo test --locked --test order_list_process -- --skip order_list_human` | PASS: 35 of 35 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

The first green run of `human_text_escapes_terminal_controls` failed only
because its long escaped payment text wrapped onto a second table line; the
fixture now uses short control strings. The watchdog test fails more often with
the ten extra child processes of T035 running in parallel (Windows process
start-up under load); it passes alone. Its assertion now reports the elapsed time.

T001-T036 are complete. T037 (output ownership and failure behavior) is next.

## T037 Output Ownership And Failure Behavior: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `de9235b`. Windows was not rerun.

### Finding

Mode already never depends on a TTY (no terminal detection in `src/`), and the
JSON path already serialized fully before one write (`parse_machine::emit`,
covered by its unit tests for an unserializable outcome and a failed write). The
defect was in the human path and the argument-error path: they used `eprintln!`
and `eprint!`, which panic when stderr cannot be written. With the stderr reader
closed, `order list --side invalid` exited 101 instead of 2.

### Red, then green

`failed_diagnostic_write_keeps_the_exit_status_and_stdout_empty`
(`tests/order_list_process.rs`) drops the child's stderr reader and runs three
human-mode failures: invalid side (2), unknown flag (2) and missing configuration (1).
Before the change it failed with exit 101 (expected 2); after it passes.

### Change

- `src/cli.rs`: `print_list` takes the stdout and stderr writers, renders the
  whole listing before the first byte and, on a failed write, exits 1 with one
  fixed line and nothing appended. Diagnostics go through `diagnose`, and
  argument errors through a plain `write_all`; neither panics on a broken pipe.
- `src/main.rs`: the final error line is written the same way.
- `src/cli.rs` unit tests: single write with the outcome's exit, failed stdout
  write, error with empty stdout, and an unwritable stderr never changing the
  result.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process failed_diagnostic` before the change | FAIL: exit 101, expected 2 |
| same, after the change | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 102, cli_functions 62, integration 14, order_list_process 37 (15.1 s), disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

`src/cli.rs` is 226 production lines. Legacy commands still print directly
(`println!`), so whole-CLI output ownership is not migrated.

## T038 US2 Acceptance: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `de9235b` plus the T037 working-tree
change. Windows was not rerun. This closes US2 only: schema discovery is not
implemented and nothing here claims it.

| US2 obligation | Automated evidence |
| --- | --- |
| Human and JSON describe the same orders | `tests/parser_orders.rs` `human_*` (13 tests): every JSON row maps to one human row with the same id, side, status, sats, currency, fiat or range, payment, premium and date; invalid dates match JSON `created_at` null |
| Headers, filters, empty wording | `human_heading_and_filter_summary_follow_the_typed_filters`, `human_empty_book_keeps_the_existing_wording` |
| Terminal-control text escaped | `human_text_escapes_terminal_controls`; JSON keeps raw text |
| Default mode independent of TTY and redirection | `order_list_human_and_json_describe_the_same_book`: default human, `--output human`, `--verbose` and closed stdin all exit 0 with piped stdout and byte-identical tables; no terminal detection exists in `src/` |
| Zero identity, order and trade-index effects | same test: fresh sandbox stays empty, existing `mcli.db` byte-identical, relay store unchanged |
| Single write path and failure behavior (T037) | `cli::tests` (4), `parse_machine::tests::failed_write_exits_1_and_appends_nothing`, `failed_diagnostic_write_keeps_the_exit_status_and_stdout_empty` |

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test parser_orders human_` | PASS: 13 of 13 |
| `cargo test --locked --test order_list_process order_list_human` | PASS: 1 of 1 |
| `cargo test --locked --test order_list_process failed_diagnostic` | PASS: 1 of 1 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 102, cli_functions 62, integration 14, order_list_process 37, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

`watchdog_kills_and_reaps_a_hung_child` passed on this Linux run; its Windows
timing flake (see T033) is still open. The human path does not show warnings.

T001-T038 are complete. T039 (US3 parser and registry tests) is next.

## T039 US3 Parser And Registry Tests: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `a7f8b9b`. Tests only; no
production code changed. The grammar matrix from T014 is reused
(`tests/cli_functions.rs`, `output_intent` and `registry`), not duplicated.

### Already covered (verified, not rewritten)

Every canonical registry mapping (`registry_matches_the_approved_mapping`,
`every_subcommand_is_registered_exactly_once`), every side and status spelling in
both cases, the removed spellings (`listorders`, `--kind`, `--mostropubkey`),
and output/help/version precedence for `order list`.

### Added

| Test | Status | Covers |
| --- | --- | --- |
| `valid_unsupported_invocations_parse_and_malformed_ones_are_invalid_arguments` | passes now | Valid legacy invocations (`getlasttradeprivkey`, `getlasttradeindex`, `restore`, `admmaintenancestatus`, `takesell --orderid`) parse for JSON and resolve to an unsupported registry entry; malformed ones (`takesell` without id or with a bad id, an unknown flag on a registered command) are JSON `invalid_arguments` with the resolved command; `order nosuch` and `nosuch --bogus` give a null command; human mode gets stderr |
| `schema_parses_with_output_at_every_depth` | ignored, red until T043 | `schema` in all six output positions, with root node options |
| `schema_takes_no_arguments_and_keeps_json_intent_on_errors` | ignored, red until T043 | extra argument, unknown flag, `order list` flags, duplicate/missing output, `--` terminator |
| `schema_help_and_version_stay_text` | ignored, red until T043 | `--help`, `help schema`, `--version` precedence with JSON selected |
| `schema_is_a_registered_machine_command_with_a_non_dotted_name` | ignored, red until T043 | `schema` entry, canonical name `schema`, read-style stages, machine commands are exactly `order.list` and `schema` |

Red: all four ignored tests fail on this checkout because there is no `schema`
subcommand (`InvalidSubcommand`). Green: with a throwaway prototype (a `Schema`
variant, a registry entry and an empty match arm) all four passed; the
prototype was reverted and is not in the tree.

### Handoff to T043

- Remove the four ignores when `schema` lands.
- Two existing registry tests will need updating then: `canonical_names_are_dotted_lowercase`
  (the name `schema` has no dot) and `only_order_list_supports_json` (`schema`
  is also machine-capable). `data-model.md` already names `schema` as the
  discovery identifier.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions` | PASS: 63 passed, 4 ignored |
| `cargo test --locked --test cli_functions -- --ignored` | FAIL as intended: 4 of 4 |
| prototype, same command | PASS: 4 of 4, then reverted |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 102, cli_functions 63 (4 ignored), integration 14, order_list_process 37, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T039 are complete. T040 (US3 process tests) is next.

## T040 US3 Process Tests: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `445e944`. Tests only; no
production code changed. All children use the T013 harness: allow-listed
environment, sandboxed HOME/MCLI_DIR/cwd, closed stdin, watchdog and reaping.

### Added (`tests/order_list_process.rs`)

| Test | Status | Covers |
| --- | --- | --- |
| `every_legacy_command_rejects_json_before_any_effect` | passes now | All 32 registered legacy commands, each with minimal valid arguments and `--output json`, including the gRPC (`admsetmaintenance`, `admmaintenancestatus`, `admcancelpending`) and secret (`getlasttradeprivkey`, `conversationkey`) paths: exit 2, one document, canonical `command`, `output_not_supported`, stages `not_attempted`/`unknown`/`not_attempted`, empty stderr, sandbox untouched, zero connections to a configured relay/RPC endpoint |
| `configuration_errors_are_stable_and_leak_nothing` | passes now | `configuration_missing` (no env, key only, relays only, blank relays) and `configuration_invalid` (bad key, bad relay, invalid CLI override despite valid env): exit 1, `order.list`, data and snapshot null, planted sentinel absent, sandbox untouched; human mode prints empty stdout and one fixed line |
| `an_unreachable_relay_is_a_connection_failure_in_the_binary` | passes now | `relay_connection_failed` from a refused loopback port |
| `schema_runs_offline_in_both_modes` | ignored, red until T043 | `schema` human (raw discovery) and JSON (`command` `schema`, same data) with absent and invalid configuration plus a reachable relay: exit 0, empty stderr, no sentinel, sandbox untouched, zero relay connections |

Already covered elsewhere and not duplicated: `invalid_arguments` and
`output_not_supported` rows (T018, T031), `relay_read_failed` and `read_timeout`
in the binary (T031), and the output-selection fallbacks (T017/T018).
`read_limit_exceeded` is covered only at the collector level
(`exceeding_the_event_cap_fails_without_truncating`) because the production
10,000-event cap is impractical to fill through a process; `internal_error` is
covered by the `parse_machine` serialization tests. Both stay open for T046.

### Red and green

The ignored test fails on this checkout with exit 2 (`unrecognized subcommand`).
A throwaway prototype (a `Schema` variant, a registry entry and a hard-coded
discovery document) made it pass; the prototype was reverted and is not in the
tree. The first prototype run exposed a test defect: `one_document` asserts the
`order.list` command, so the schema test now checks the envelope itself.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process -- every_legacy configuration_errors unreachable_relay_is` | PASS: 3 of 3 in 0.2 s |
| `cargo test --locked --test order_list_process -- --ignored schema_runs` | FAIL as intended; PASS against the prototype |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 102, cli_functions 63 (4 ignored), integration 14, order_list_process 40 (1 ignored), disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T040 are complete. T041 (discovery DTOs) is next; T043 removes the five
`red until T043` ignores.

## T041 Discovery DTOs: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `9d185c2`.

### Change

New `src/cli/schema.rs` (118 production lines), declared and re-exported from
`src/cli.rs`: `DiscoveryData` (`schema_version` `1`, `executable_version` from
`CARGO_PKG_VERSION`, Draft 2020-12 `schema_dialect`, `commands`),
`CommandDescriptor` (`name`, `cli_path`, `output_modes`, `input_schema`,
`output_schema`, `required_configuration`), `PresentationMode`, `CommandSchemas`
and `discovery(schemas)`, which describes every registry entry sorted by canonical
name. It is built from the registry and constants only: no environment value,
identity or network is read. `order.list` names `MOSTRO_PUBKEY` and `RELAYS`;
`schema` gets an empty list; legacy entries get `null`. Schema documents are
plain JSON objects, so there is no schema of a schema. No dependency was added.

Not in scope here: the schema documents themselves (T042) and the `schema`
subcommand and registry entry (T043). The builder takes the documents from the
caller, so T042 supplies them and a machine command given none serializes
both as `null`; T042 must make that case impossible for `order.list`.

### Evidence

Seven unit tests in `src/cli/schema.rs`: header constants and package version;
every registered command described once, sorted by name, with the registry's
`cli_path`; legacy entries human-only with null schemas and configuration;
`order.list` with both modes, schemas and configuration names; an unsupplied
schema stays null; a synthetic `schema` entry gets an empty configuration list
and sorts after `order.list`; and an exact serialized legacy descriptor with every
key present including nulls.

Red: with a `todo!` body all 7 failed. Green after the implementation.
A unit test that set the environment variables was dropped because it mutates
process state shared with other tests; the process tests of T040 and T046 prove
that discovery ignores configuration values.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib cli::schema` | red 0 of 8 (before dropping one test), then PASS 7 of 7 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 109, cli_functions 63 (4 ignored), integration 14, order_list_process 40 (1 ignored), disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T041 are complete. T042 (generated schemas) is next.

## T042 Generated Schemas: 2026-10-27

Native Linux, rustc 1.97.0, on `develop` after `f863231`.

### Change

- `src/outcome.rs`, `src/outcome/order.rs`: schemars `JsonSchema` derives on the
  envelope pieces and the order DTOs. Fields the derive cannot describe use
  `schema_with` helpers for foreign core types (side, status), decimal strings,
  UUIDs, node hex, RFC3339 `Z` times and the fixed snapshot values.
- New `src/outcome/schema.rs` (223 production lines): `output_schema` builds a
  Draft 2020-12 document per command from the Serialize contract, so
  nullable fields are `required` and typed with `null`. It has three branches:
  success (command, `ok`, data, null error, read stages, snapshot rule), failure of
  the resolved command (only its listed codes, null data) and failure before
  resolution (null command, `invalid_arguments`, `not_attempted`/`unknown`/
  `not_attempted`). `uncertain` is excluded in the pilot. Unknown fields stay allowed.
- `src/cli/schema.rs`: `input_schema` reads the arguments of a command from
  Clap (names, help, required, possible values, case-insensitivity), adds the
  shared validator's `pending` default, excludes `--output`/help/version and
  rejects extra properties; `command_schemas` supplies both documents for
  `order.list`. `SCHEMA_VERSION` and the dialect are now defined once, in `outcome`.
- Dev-only dependency `regex` 1 (default features off, `std`): already in the
  lockfile, so `Cargo.lock` gains one dependency line and no package; used only
  to run the generated patterns in tests.

### Evidence

- `outcome::schema` (8 tests): envelope keys and dialect; real serialized
  success, failure, snapshot-bearing failure and unresolved outcomes conform
  to the generated schema, checked by a small in-test schema checker;
  19 contradictory or malformed outcomes do not (wrong or `uncertain` status,
  data with an error, wrong command, missing key, `-0`, numeric amount, bad
  UUID or offset time, snapshot rules, a code the command cannot produce);
  nullable order fields required and nullable; decimal, UUID, node-key and
  time patterns accept and reject the documented cases; error, warning, stage,
  side and status enums equal the contract and `STATUSES`; a command
  without snapshot evidence requires a null one.
- `cli::schema` (4 more tests): listing input equals Clap's arguments with the
  shared enums and default; `neworder` requirements follow Clap; unknown
  paths give none; every machine-capable registry entry has generated schemas
  and legacy entries have none; discovery carries the listing documents.
- Teeth: loosening the decimal pattern to `^-?[0-9]+$` failed
  `decimal_pattern_accepts_only_canonical_integers` and
  `contradictory_or_malformed_outcomes_do_not_conform`; it was restored.
- Red note: the first run was a compile failure (the DTOs did not implement
  `JsonSchema`), not a behavioral failure; the mutation above is the evidence
  that the assertions can fail.

### Limits and handoff

- The signed 64-bit range of decimal strings cannot be a pattern; it stays a
  producer rule covered by `money_fields_are_canonical_signed_decimal_strings`.
- `Snapshot` constants (50, 10,000) are `const` in the schema; process tests that
  shrink limits emit non-conforming snapshots only in tests.
- `DiscoveryData` has no `JsonSchema` derive yet and `schema` has no registry entry;
  T043 adds both, and `every_machine_command_has_generated_schemas_and_legacy_has_none`
  will then require a schema for it.
- The conformance checker is test-local. T045 validates the proposed examples
  and the checked-in fixtures and decides whether to share it or add a validator.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --lib outcome::schema cli::schema` | PASS: 8 + 11 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 121, cli_functions 63 (4 ignored), integration 14, order_list_process 40 (1 ignored), disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T042 are complete. T043 (offline `schema` dispatch) is next.

## T043 Offline Schema Dispatch: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `90fa5c9`.

### Change

- `src/cli/commands.rs`: a `Schema` subcommand with no arguments.
  `src/cli/registry.rs`: its entry, canonical name `schema`, read stages. In
  `legacy.rs` it is unreachable, like `order list`; `run_rpc` already ignores it.
- `src/cli.rs`: `schema` is dispatched right after parsing, before the
  public listing, `install_rustls_crypto_provider`, `run_rpc`, the logger and
  `init_context`. Human mode (default or explicit) prints the discovery
  document as pretty JSON through `print_discovery`, which serializes fully
  before one write and exits 1 with one fixed line on a failed write. `--output json`
  wraps the same data in the standard envelope through `emit`.
- `src/cli/schema.rs`: `DiscoveryData` and its parts derive `JsonSchema`;
  `command_schemas` now also generates `schema`'s documents (input from Clap,
  output from `DiscoveryData`, errors `invalid_arguments` and `internal_error`).

### Evidence

- The five tests ignored as `red until T043` pass with their ignores removed:
  four in `tests/cli_functions.rs` (schema in all output positions, no
  arguments plus JSON intent on errors, help/version text, registry entry) and
  `schema_runs_offline_in_both_modes` in `tests/order_list_process.rs`. That
  process test runs `schema` in both modes with no configuration and with an
  invalid key, a planted sentinel, an RPC token and a reachable relay: exit 0,
  empty stderr, identical data in both modes, no sentinel, sandbox untouched and
  zero relay connections.
- Registry tests were updated, as T039 forecast: the approved mapping gains
  `schema`; the dotted-name check skips it; `only_order_list_and_schema_support_json`.
  `every_machine_command_has_generated_schemas_and_legacy_has_none` now covers
  `schema` and passes. The legacy-rejection process test counts two machine commands.
- New unit tests in `src/cli.rs`: one pretty document equal to the typed data and
  listing `schema` and `order.list`; a failed write exits 1 with one fixed line and
  an unwritable stderr does not change the result.
- Manual check: `mostro-cli schema` prints the pretty document and
  `mostro-cli --output json schema` the envelope.

Not claimed: the checked-in `docs/schemas/*` fixtures and their drift checks
(T045), the full stable-error mapping (T044) and Windows runs.

### Runs

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 123, cli_functions 67, integration 14, order_list_process 41, disputes 9, dms 19, parser_orders 110, util 17; no ignored tests |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

Production lines: `cli.rs` 271, `cli/schema.rs` 219, `cli/registry.rs` 94.

T001-T043 are complete. T044 (safe error mapping) is next.

## T044 Safe Error Mapping: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `9245045`.

### Audit

Each stable code was traced from its source to the wire. The mapping in
`src/outcome.rs` (codes, exits, fixed prose, stage sets), `src/msg/mod.rs`
(`configuration_*`, `relay_connection_failed`) and `src/msg/receive_msg.rs`
(`relay_read_failed`, `read_timeout`, `read_limit_exceeded`, failures carrying
the snapshot and cleanup warnings) was already complete and tested
(`outcome::tests`, T021/T026 collector tests, T031/T040 binary tests), as was
the argument-error identity in `src/cli/invocation.rs` and `report`. One gap
was found.

### Defect and fix

`run()` turned a failure to bind parsed arguments to the typed grammar into
an `anyhow` error: stderr text, empty stdout and exit 1 even with `--output json`,
which breaks the one-document promise for a controlled outcome. It is now
`unbound` in `src/cli.rs`: JSON mode emits an `internal_error` envelope with the
resolved command identity and stage evidence (or a null command), human mode
prints one fixed line, both exit 1. `rejection` now delegates to `reject_to`, which
takes the writers. The case cannot occur through the real grammar, so
`unbindable_arguments_are_an_internal_error_in_the_selected_mode` covers it with
parsed matches directly; its first run did not compile (no `unbound`), so it is
a compile-level red, not a behavioral one.

### Added tests

- `an_unwritable_stdout_exits_1_with_one_fixed_line` (process): with the stdout
  reader closed, an argument error (2), a removed spelling (2), a `schema`
  success (0) and the human discovery document all exit 1 with exactly one fixed
  stderr line; the sandbox stays empty. This is the documented
  "broken stdout write" row, previously covered only at unit level.
- `schema_argument_errors_keep_the_resolved_command` (process): three malformed
  `schema` invocations return exit 2, command `schema`, `invalid_arguments`, null
  data, read stages, one document, no planted sentinel and no effects.

### Not covered here

`read_limit_exceeded` and `internal_error` still have no binary-level case: the
production 10,000-event cap needs a relay that ignores the requested limit, and
`internal_error` cannot be induced through a real run. Both stay with the
collector and serialization tests and with T046.

### Runs

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 14, order_list_process 43, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

`src/cli.rs` is 316 production lines.

T001-T044 are complete. T045 (checked-in schema fixtures and drift checks) is next.


## T045 Schema Fixtures And Drift Checks: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `8ca6e3e`.

### Fixtures

- `docs/schemas/discovery-v1.json`: the raw `DiscoveryData` (no envelope) that
  `mostro-cli schema` prints: 34 commands, 35.6 KB.
- `docs/schemas/order-list-v1.json`: the `order.list` output schema, equal to that
  descriptor's `output_schema`: 12.1 KB.
- Both were produced by the shipped binary, not by hand:
  `cargo run -q -- schema > docs/schemas/discovery-v1.json` and
  `cargo run -q -- --output json schema | jq '.data.commands[] | select(.name=="order.list") | .output_schema' > docs/schemas/order-list-v1.json`.
  `executable_version` is part of the fixture, so a package version bump fails
  the drift check until the fixtures are regenerated. That is intended.

### Change

- New `offline_discovery()` in `src/cli/schema.rs`; the `schema` command, the
  fixtures and the drift tests all use this one path (registry, Clap grammar and
  owned DTOs). `cli.rs` calls it instead of building the document itself.
- New `tests/common/schema_check.rs`: the schema checker from T042, now one file
  shared by the `outcome::schema` unit tests (through `#[path]`) and the
  integration tests, so there is no second copy and no new dependency.

### Evidence (`tests/integration_tests.rs`, 8 new tests)

| Test | Covers |
| --- | --- |
| `discovery_fixture_matches_the_generated_document` | parsed fixture equals the regenerated document; the failure message prints the regeneration commands |
| `listing_schema_fixture_matches_the_generated_schema` | same for the listing schema |
| `discovery_fixture_is_raw_discovery_data_not_an_envelope` | exactly the four DiscoveryData keys, sorted commands, machine commands are `order.list` and `schema` |
| `proposed_listing_examples_conform_to_the_checked_in_schema` | the four `order.list` examples in `examples.json` conform; exits agree; every decimal field is a canonical signed 64-bit integer |
| `the_legacy_rejection_example_matches_its_constructor_and_is_not_a_listing` | `unsupported_secret_command` equals the owned constructor's output and is not a listing outcome |
| `examples_stay_proposals_separate_from_generated_output` | `contract_status` stays `proposed`; generated files live only under `docs/schemas` |
| `invalid_types_nulls_enums_and_amounts_do_not_conform` | 19 edits of the large-amount example are rejected: wrong types, nulls, unknown enum values, `-0`/`+5`/`015`/`1.5`/`1e3`, a missing nullable key, a claimed complete history, an error with data |
| `the_signed_64_bit_range_is_a_semantic_rule_the_pattern_cannot_state` | range boundaries pass; one past either end passes the pattern but fails the semantic check; a 21-character value fails the schema |

The tests only read the fixtures; none rewrites them.

Teeth: with `executable_version` changed in the discovery fixture and the schema
version constant changed in the listing fixture, five tests failed (both drift
checks, the example conformance, the mutation test and the range test); the
fixtures were restored and all passed again.

CI wiring of these checks and of the Windows runs is T051.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test integration_tests` | PASS: 22 (8 new) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS after boxing a complex test type |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 22, order_list_process 43, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T045 are complete. T046 (full configuration/argument/read matrix) is next.

## T046 Full Process Matrix: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `c6cd503`. Tests only; no
production code changed. Harness as in T013.

### Matrix

The configuration, argument, unsupported-command and secret cases were
already covered by T018, T031, T032, T040 and T044 and were not repeated. The
gaps found were failures after partial relay results, the event cap through a
real process, every output position for a success, and a combined secret sweep.

| Test | Covers |
| --- | --- |
| `read_failures_are_stable_in_every_mode_without_retries` | Four failing reads, each run in JSON, human and verbose-JSON at once: `closed`, `answering then closed` (`relay_read_failed`, snapshot kept), `answering then refused` (`relay_connection_failed`, no snapshot), `answering then silent` (`read_timeout` after the 15 s budget, snapshot kept). Each checks exit 1, one document, `order.list`, null data, snapshot presence, the human line `error: <message>` with empty stdout, only the allow-listed verbose line, an untouched sandbox and exactly three connections per relay for three runs |
| `exceeding_the_event_cap_fails_the_binary_without_truncating` | Eleven loopback relays each answering a query with 1,000 distinct signed orders (limit and relay caps lifted): `read_limit_exceeded`, null data, snapshot `received_events` exactly 10,000, same stream checks |
| `every_output_position_returns_the_same_listing` | Six spellings of `--output json` (root, intermediate, leaf, space and equals forms): exit 0, one document each, identical command, status, data, error, warnings and event count, an untouched sandbox, one read per run |

Secrets planted in every failing run: a token in each relay URL query,
`ADMIN_NSEC`, `MOSTRO_RPC_TOKEN` and `NSEC`. None appears in stdout, stderr or the
verbose line; no URL host appears in stdout or the allow-listed lines.

### Findings

- The SDK reports each relay event twice (`Event` and `Message` notifications),
  so a relay that ignores the requested limit fails with `relay_read_failed`
  after about 1,020 events, before the 10,000-event cap can apply to it. The cap
  is therefore reachable only across several relays, which is how the test reaches it.
- Retry check limits: the "exactly one connection per relay" assertion does not
  detect a library change that enables SDK reconnection (checked: switching
  `reconnect(false)` to `reconnect(true)` still passed), because reconnection only
  follows a dropped connection and none of these rows drops one. The disconnect-
  before-EOSE case remains covered only at the collector level
  (`a_relay_disconnecting_before_eose_fails_the_read`), without a connection count.
- `internal_error` still has no binary-level case; it cannot be induced through a
  real run and stays with the serialization and unbound-argument unit tests.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test order_list_process -- exceeding_the_event_cap read_failures_are every_output_position` | PASS: 4 of 4 in 15.1 s (the timeout row waits its 15 s budget) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 22, order_list_process 46, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T046 are complete. T047 (US3 acceptance run) is next.

## T047 US3 Acceptance: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `d0d0099`. No code changed in
this task. It covers US3 (offline discovery and stable errors) on this platform
only; Windows, CI wiring and the final reconciliation remain open.

### Coverage

| US3 obligation | Automated evidence |
| --- | --- |
| Offline discovery in both modes, no configuration, identity, connection or stdin | `order_list_process::schema_runs_offline_in_both_modes`: absent and invalid configuration, planted sentinel, reachable relay, zero connections, untouched sandbox; identical data in both modes |
| `schema` grammar, help/version, registry entry | `cli_functions`: schema at every output depth, no arguments plus JSON intent on errors, help/version text, registry mapping (34 entries, 2 machine-capable) |
| Versions | `schema_version` `1`; `executable_version` `0.16.3` in the fixture equals `Cargo.toml`; the drift test fails on any change |
| Schema drift | `integration_tests`: both checked-in fixtures equal the regenerated documents; the failure message prints the regeneration commands; corrupting either fixture failed five tests (T045) |
| Field and encoding rules | `integration_tests`: the four proposed `order.list` examples conform; 19 malformed variants do not; decimal strings are canonical signed 64-bit integers, with the range rule checked semantically |
| Early guard for unmigrated commands | `every_legacy_command_rejects_json_before_any_effect`: all 32 legacy commands incl. gRPC and secret paths, exit 2, `output_not_supported`, zero connections |
| Stable errors | table below |
| Secrets | planted sentinels in arguments, environment, relay URLs and tokens absent from every stream in the T040, T032 and T046 runs |

| Code | Exit | Binary-level evidence |
| --- | --- | --- |
| `invalid_arguments` | 2 | contract decision table (T018), `order_list_json_uses_read_stages`, `schema_argument_errors_keep_the_resolved_command` |
| `output_not_supported` | 2 | `every_legacy_command_rejects_json_before_any_effect` |
| `configuration_missing`, `configuration_invalid` | 1 | `configuration_errors_are_stable_and_leak_nothing` |
| `relay_connection_failed` | 1 | `an_unreachable_relay_is_a_connection_failure_in_the_binary`, `read_failures_are_stable_in_every_mode_without_retries` |
| `relay_read_failed` | 1 | `order_list_binary_read_failures_return_no_rows`, the T046 matrix |
| `read_timeout` | 1 | `order_list_binary_times_out_on_a_silent_relay`, the T046 matrix |
| `read_limit_exceeded` | 1 | `exceeding_the_event_cap_fails_the_binary_without_truncating` |
| `internal_error` | 1 | unit level only: `parse_machine` serialization failure and `unbound_arguments_are_an_internal_error_in_the_selected_mode` |

Also covered: the unwritable stdout (exit 1, one fixed line), unwritable stderr
(result unchanged) and help/version text exceptions.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test cli_functions` | PASS: 67 |
| `cargo test --locked --test integration_tests` | PASS: 22 |
| `cargo test --locked --test order_list_process` | PASS: 46 in 15-16 s |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 22, order_list_process 46, disputes 9, dms 19, parser_orders 110, util 17 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

### Still open (kept, not waived)

- `internal_error` has no process-level case; it cannot be induced through a real run.
- A relay disconnecting before EOSE has no connection-count check in the binary
  (the T046 retry assertion does not detect SDK reconnection).
- NIP-42 AUTH is never exercised by the loopback relays.
- Windows: no native run; the watchdog test was timing-sensitive there (T033).
- Only loopback relays were used; no public relay or live node.
- Docs, CI drift wiring and requirement reconciliation: T048-T055.

T001-T047 are complete. T048 (architecture documentation) and T049 (command
documentation) are next and may run in parallel.

## T048-T049 Documentation: 2026-10-28

Documentation only; no Rust, fixture or test file changed. Per the constitution
these tasks use link, template and statement checks instead of the code gates.

### T048 `docs/architecture.md`

- New "Boundaries Implemented By The Order-List Pilot" section: a call diagram
  from `main` through `cli::run`, a responsibility table for the CLI, discovery,
  operation, public-read, outcome and rendering modules, the rules that hold today,
  and a "Legacy Coexistence" subsection. It states that no database model was added,
  that the transport migration is not complete (one door, `msg::fetch_msg`, for
  public reads only; no shared `send_msg`/`receive_msg`; no write command returns an
  outcome) and that the client is private to `msg::Session` but not yet a crate-wide
  rule (task T050).
- The T002 table and T011 update are replaced by one recount (2026-10-28, at
  `65db631`) produced by a single script over all 62 files of `src/`:
  13,124 physical, 3,106 test-only and 10,018 production lines. Four files exceed
  400 production lines (`parser/dms.rs` 1,210, `db.rs` 704, `util/messaging.rs`
  556, `cli/take_dispute.rs` 403); every file created or migrated by the pilot is at
  most 316 (`cli.rs`). This supersedes the figures T019 found inconsistent.
- Imports rechecked: the same four function-local production imports remain and
  the pilot added none. Qualified paths in migrated files were not audited, and
  the section says so.
- Stale statements fixed: `list_orders.rs`, outcome and `commands/` described as
  stubs, the registry location, `run()` behaviour and the new-command checklist.

### T049 `docs/commands.md` and `README.md`

- `commands.md`: `--mostro-pubkey` (old spelling removed), `--output` scope, a
  Discovery section for `schema`, a "Machine output (schema v1)" section
  (single document, encodings, listing semantics, limits, the nine error codes
  with exits, help/version/write-failure exceptions, versioning and what the
  contract excludes) and `order list` replacing `listorders` with its migration
  notes. It links the generated `docs/schemas/*.json` as the command reference
  and states that the prose itself is hand-maintained.
- `README.md`: examples, command reference, global flags and troubleshooting moved
  to `order list` and `--mostro-pubkey`; a "Machine-readable output" section with
  the migration note; and a warning that the existing MCP keeps using the old
  spellings and must stay on the CLI revision its own README names, with no
  compatible release pair claimed.
- Corrected two README statements that would have become wrong: `order list` no
  longer creates the user or database, so the first-command and database-recovery
  steps now use `getdm` (read-only on the network; it opens the database), which
  was read in the code but not run, to avoid creating state on this machine.

### Checks

| Check | Result |
| --- | --- |
| Relative links and anchors in `architecture.md`, `commands.md`, `README.md` | none broken |
| `rg listorders\|mostropubkey` in README and `commands.md` | only the intended migration notes remain |
| `order list --help`, `schema --help` and the root help match the documented flags | confirmed by reading the built binary's help |
| Documented examples `schema`, `--output json schema` and an invalid `order list --side` | run against the built binary |
| Inventory script totals match the T033 per-file figures for the migrated files | yes (for example `msg/mod.rs` 160, `cli/schema.rs` 228) |

Known documentation debt, not addressed here: `docs/commands.md` still lists some
legacy flag spellings (for example `--order-id`, `--fiat-code`) where the binary
uses `--orderid` and `--fiatcode`.

T001-T049 are complete. T050 (automated size, import and boundary checks) is next.

## T050 Source Boundary Checks: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `0cefe73`.

### Change

- `tests/integration_tests.rs`, module `source_boundaries` (7 tests), reading `src/`:
  - **Size** (`production_files_respect_the_size_limit_and_the_frozen_exceptions`): every file is at most 400 production lines (comments and blanks counted) except four legacy files frozen at their measured sizes: `parser/dms.rs` 1,210, `db.rs` 704, `util/messaging.rs` 556, `cli/take_dispute.rs` 403. Growth fails; a frozen file that drops under the limit fails until its exception is removed.
  - **Exclusion rule** (`counter_applies_the_documented_exclusions`, `production_code_after_a_test_module_is_rejected`): the counter excludes only a trailing `#[cfg(test)] mod` and two-line `#[cfg(test)]` imports, and rejects any file where the test module closes before the last nonblank line.
  - **Imports** (`production_imports_are_at_the_top_except_frozen_legacy`): no function-local `use` and no `use` after the first item, except the four known legacy local imports.
  - **Transport ownership** (`network_calls_stay_with_their_owner_and_do_not_grow`): `.send_event(`, `.fetch_events(`, `.subscribe(` and `.add_relay(` may appear only in the listed files and at most the frozen count; `msg/` owns the public read, the rest is legacy transport that a later migration moves behind it. There is still no owner for publication.
  - **No I/O** (`renderers_operations_and_outcomes_perform_no_io`): `parse_human/`, `parse_machine.rs`, `commands/`, `outcome*`, `cli/schema.rs` and `cli/registry.rs` contain no printing, process exit, filesystem, environment, storage, client or (where relevant) network use.
  - **Export** (`the_public_read_client_is_not_exported_from_msg`): no public item of `msg/mod.rs` names the session or client.
- `src/msg/mod.rs`: three rustdoc examples run by `cargo test --doc`: `Session` and the private `orders` module cannot be imported (`compile_fail,E0603`), and the exported surface compiles.
- `.github/workflows/ci.yml`: a named "Source boundary checks" step in the `test` job. It repeats part of `cargo test`, so a failure names the rule that broke. Existing jobs are unchanged; drift and Windows wiring remain T051.
- `src/cli/rate_user.rs`: the only violation the new import check found (a `use` block after a `const`) was fixed by moving the block above the constant; no behavior change.
- `tests/order_list_process.rs`: the T044 broken-pipe tests are now deterministic. They raced under load (one failure in about six full runs): the parent closed the pipe after spawning and the child sometimes wrote first. The read end is now closed before the spawn (`io::pipe`); ten full runs afterwards were clean.

### Evidence

Mutations, each caught by exactly the intended check and then reverted:
a `println!` in `commands/order/list.rs` (no-I/O test), a `".send_event("` string in `util/misc.rs` (transport test), 60 added lines in `util/messaging.rs` (size test), a function-local import in `cli/schema.rs` (import test), and `pub struct Session` (the first compile-fail doctest).

Not claimed: the legacy code is not migrated. `ephemeral_client` is still `pub(crate)` and used by legacy `util/net.rs`, so "no client outside `msg`" is a rule for the public read, enforced by the export and doctest checks, not a crate-wide one. Qualified-path imports are not checked. The scan is textual: it can be fooled by a call spelled differently.

### Runs

| Command | Result |
| --- | --- |
| `cargo test --locked --test integration_tests source_boundaries` | PASS: 7 |
| `cargo test --locked --doc` | PASS: 3 |
| `cargo test --locked --test order_list_process` ten times | PASS 10 of 10 (46 tests each) |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 29, order_list_process 46, disputes 9, dms 19, parser_orders 110, util 17, doctests 3 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

T001-T050 are complete. T051 (CI wiring of drift checks and platform tests) is next.

## T051 CI Wiring: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `3f745ac`. **No workflow has run
on GitHub**: the changes were checked for YAML validity and, through tests, for
their content only. The new Windows job is unproven until it runs.

### Existing jobs inspected first

`ci.yml` (on every push): `fmt`, `clippy`, `test` (ubuntu, `cargo test`), `build`,
`msrv` (1.94.0). `rust.yml` (on version tags): `changelog`, `test` (`cargo test
--locked`), `matrix`, `build` (cross, incl. a `windows-gnu` cross build; no native
Windows test), `publish` (needs `test` and `build`), `release`. All were kept,
with their names, ids and `needs`.

### Change

- `ci.yml`, job `test`: a named step "Schema drift checks" running
  `cargo test --locked --test integration_tests schema_fixtures`. The tests compare
  the checked-in `docs/schemas/*.json` with what the binary generates and fail
  when they differ; nothing in a workflow regenerates, commits or pushes them.
- `ci.yml`, new job `pilot-windows` (`windows-latest`, `permissions: contents:
  read`, no secrets, no shell steps): `cargo test --locked`, which runs the process,
  grammar, schema-drift and boundary tests natively. It is independent of `build`
  and of releases.
- `rust.yml`, job `test` (which gates `publish`): the same drift step, so a release
  with stale fixtures, for example after a version bump, fails before publishing.
  I did not make `publish` wait for the Windows job: it has not run yet, and
  blocking releases on it is a maintainer decision after it passes once.
- `tests/integration_tests.rs`: the T045 tests moved into `mod schema_fixtures`
  (so one filter selects them) and four `ci_wiring` tests read the workflow files:
  the drift, boundary and doctest steps and the Windows job exist, with read-only
  permissions and no secrets or shell scripts; the release workflow runs the drift
  step before `publish`; `fmt`, `clippy`, `test`, `build` and `msrv` remain with their
  commands; no workflow line mentions `docs/schemas` or runs `git add`, `git
  commit`, `git push` or `cargo run`.
- `tests/order_list_process.rs`: `watchdog_kills_and_reaps_a_hung_child` now uses
  a 6 s watchdog instead of 2 s. Its recorded Windows flake (T033, T036) is
  consistent with a slow child start missing the 2 s window before the assertion that
  it connected; unverified on Windows.

### Evidence

Each wiring test fails when its rule is broken (all reverted afterwards): replacing
the drift command, moving the Windows job to Ubuntu, appending a `git commit` step,
renaming the `msrv` job. Both workflow files parse with a YAML loader and list the
expected jobs (`ci.yml`: fmt, clippy, test, pilot-windows, build, msrv).

| Command | Result |
| --- | --- |
| `cargo test --locked --test integration_tests schema_fixtures ci_wiring` | PASS: 8 + 4 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS: lib 124, cli_functions 67, integration 33, order_list_process 46, disputes 9, dms 19, parser_orders 110, util 17, doctests 3 |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

### Open

- The first GitHub run of `pilot-windows` may expose Windows-only failures; none can
  be assumed absent. This is the work of T052, which also needs a native host.
- `ci.yml`'s `test` and `clippy` jobs do not pass `--locked`; left as they were.
- Whether the Windows job should be a required check or gate releases is undecided.

T001-T051 are complete. T052 (native Windows and Linux acceptance run) is next and
is blocked on a native Windows run.

## T052 Quickstart Acceptance: 2026-10-28

**Linux: run. Windows: not run, blocked** (no native Windows host in this
environment; the CI job added in T051 has not run on GitHub). The task is therefore
**not complete for SC-005**; it is checked in the task list only for what it records
and the Windows result must be added separately. Native Linux, rustc 1.97.0, on
`develop` after `4feddba`, loopback relays only, generated keys, no public node.

### Quickstart rows (all 26, Linux)

Every row of the [acceptance matrix](../specs/001-order-list-json/quickstart.md#acceptance-matrix)
has at least one passing automated test; the named tests exist in the current
`cargo test -- --list` output (428 tests).

| # | Scenario | Tests |
| --- | --- | --- |
| 1 | Mixed signed events, same-ID updates, latest state nonmatching | `listing_selects_latest_matching_states_across_relays`; `order_list_binary_selects_rows_in_both_modes`; `list_contract::same_order_time_ties_keep_the_lowest_event_id` |
| 2 | Duplicate events and equal timestamps | `duplicate_events_and_same_order_timestamp_ties_keep_lowest_event_id`; `list_contract::cross_order_time_ties_have_no_promised_order`; `rows_sort_newest_first_without_promising_cross_order_tie_order` |
| 3 | Side/status case, whitespace, empty currency | `list_contract::side_and_status_are_case_insensitive::case_1`; `list_contract::untrimmed_or_unknown_side_and_status_are_invalid_arguments::case_1`; `list_contract::currency_is_uppercased_without_trimming_or_nulling::case_1`; `list_contract::default_filters_are_pending_with_null_side_and_currency`; `output_intent::every_side_and_status_spelling_parses_and_removed_spellings_fail` |
| 4 | Invalid/missing tags, incomplete ranges, zero, i64 extremes | `list_contract::malformed_events_are_skipped_without_failing_the_read`; `list_contract::incomplete_or_unordered_ranges_stay_as_decoded::case_1`; `list_contract::zero_sats_stays_the_market_priced_zero`; `list_contract::money_fields_are_canonical_signed_decimal_strings::case_1`; `permissive_numeric_tags_and_range_display::case_1` |
| 5 | Invalid calendar timestamp | `list_contract::far_future_events_saturate_with_one_warning`; `list_contract::human_dates_are_the_utc_minute_or_invalid_date_like_json::case_1` |
| 6 | Empty relay with matching EOSE | `order_list_binary_empty_results_are_successes` |
| 7 | Nonempty events but no matching orders | `order_list_binary_empty_results_are_successes` |
| 8 | CLOSED after accepted REQ | `a_closed_relay_fails_the_whole_read`; `order_list_binary_read_failures_return_no_rows`; `read_failures_are_stable_in_every_mode_without_retries` |
| 9 | Silent relay after REQ | `a_silent_relay_times_out_and_cleanup_stays_bounded`; `order_list_binary_times_out_on_a_silent_relay` |
| 10 | One relay completes, another fails or times out | `read_failures_are_stable_in_every_mode_without_retries`; `order_list_binary_read_failures_return_no_rows`; `an_unreachable_relay_fails_before_the_query` |
| 11 | Duplicate relay URL, unrelated subscription/event, event after EOSE | `duplicate_relays_and_events_count_once`; `events_failing_verification_or_the_filter_are_dropped`; `events_after_a_relays_eose_are_ignored` |
| 12 | Excess unique events, lag or disconnect before EOSE | `exceeding_the_event_cap_fails_without_truncating`; `exceeding_the_event_cap_fails_the_binary_without_truncating`; `a_relay_filling_its_notification_buffer_fails_the_read`; `a_relay_disconnecting_before_eose_fails_the_read` |
| 13 | Output flag at root/intermediate/leaf and equals form | `every_output_position_returns_the_same_listing`; `output_intent::output_is_selected_at_every_depth_and_defaults_to_human` |
| 14 | Default or explicit human, redirected stdout | `order_list_human_and_json_describe_the_same_book`; `list_contract::human_rows_carry_the_json_business_fields` |
| 15 | Missing/invalid args with clear JSON intent | `json_intent_gets_one_error_envelope_before_any_effect`; `order_list_json_uses_read_stages`; `output_intent::contract_decision_table_rows`; `schema_argument_errors_keep_the_resolved_command` |
| 16 | Flags/commands inside values, hyphen values, clusters, variadics, -- | `output_intent::values_resembling_flags_or_commands_are_not_declarations`; `output_intent::arities_and_hyphen_values_follow_clap`; `output_intent::short_clusters_and_colliding_shorts_follow_scope`; `output_intent::terminator_ends_option_parsing`; `output_intent::non_unicode_values_keep_json_intent` |
| 17 | Invalid/missing/duplicate output | `output_intent::duplicate_and_missing_declarations_are_invalid`; `argument_errors_exit_2_with_safe_stderr_before_any_effect` |
| 18 | Valid help/version with output json | `help_and_version_stay_text_on_stdout`; `output_intent::clap_decides_help_and_version_precedence`; `output_intent::schema_help_and_version_stay_text` |
| 19 | Missing/invalid public config | `configuration_errors_are_stable_and_leak_nothing`; `msg::tests::missing_values_are_configuration_missing`; `msg::tests::empty_or_invalid_cli_values_do_not_fall_back` |
| 20 | Registered legacy JSON incl. RPC and secret paths | `every_legacy_command_rejects_json_before_any_effect`; `output_intent::valid_unsupported_invocations_parse_and_malformed_ones_are_invalid_arguments` |
| 21 | Removed listorders/--mostropubkey | `output_intent::every_side_and_status_spelling_parses_and_removed_spellings_fail`; `argument_errors_exit_2_with_safe_stderr_before_any_effect` |
| 22 | Offline schema with absent or invalid config | `schema_runs_offline_in_both_modes`; `output_intent::schema_parses_with_output_at_every_depth` |
| 23 | Verbose and fresh/existing/unusable identity locations | `order_list_binary_ignores_existing_and_unusable_identity_locations`; `order_list_diagnostics_are_allow_listed_in_verbose_mode` |
| 24 | Planted secret sentinels | `order_list_diagnostics_are_allow_listed_in_verbose_mode`; `read_failures_are_stable_in_every_mode_without_retries`; `configuration_errors_are_stable_and_leak_nothing`; `output_intent::failures_carry_no_argument_text` |
| 25 | Serialization and failing output writer | `an_unwritable_stdout_exits_1_with_one_fixed_line`; `failed_diagnostic_write_keeps_the_exit_status_and_stdout_empty`; `parse_machine::tests::failed_write_exits_1_and_appends_nothing`; `parse_machine::tests::unserializable_outcome_becomes_one_internal_error`; `cli::tests::failed_human_write_exits_1_with_one_fixed_line` |
| 26 | Discovery, generated schemas, owned DTO examples | `schema_fixtures::discovery_fixture_matches_the_generated_document`; `schema_fixtures::listing_schema_fixture_matches_the_generated_schema`; `schema_fixtures::proposed_listing_examples_conform_to_the_checked_in_schema`; `outcome::schema::tests::real_outcomes_conform_to_their_command_schema` |

### Offline manual smoke check (quickstart)

Run on the built binary with an empty environment, `HOME` and `MCLI_DIR` pointing
at a fresh temporary directory and a fresh working directory:

| Command | Result |
| --- | --- |
| `mostro-cli schema` | exit 0, 35,596 bytes of pretty discovery JSON, empty stderr |
| `mostro-cli schema --output json` | exit 0, one `ok` outcome for command `schema`, empty stderr |
| `mostro-cli order list --side invalid --output json` | exit 2, one `invalid_arguments` outcome for `order.list`, empty stderr |

No file was created in either directory. This is a convenience check; the
process tests are the evidence that nothing is read or written.

### Time, cap, cleanup, secret and no-effects evidence

- **Time**: the production read budget is observed once, by the silent-relay row
  (15 s plus cleanup); `order_list_process` takes 15-17 s wall, the other suites
  under 0.4 s each.
- **Cap**: the 10,000-unique-event cap fails the read with `read_limit_exceeded`
  both in the collector and in the binary (eleven relays, 10,000 events kept); the
  2,048-notification capacity fails a relay that floods before EOSE.
- **Cleanup**: bounded cleanup after a silent relay, a connection failure and a
  cancelled read (`wait_idle` on the relays); a cleanup timeout adds a warning and
  never replaces the original error.
- **Secrets**: planted sentinels in arguments, environment, relay URLs, tokens and
  untrusted relay text never reach stdout, stderr or the verbose line, in JSON,
  human and verbose modes.
- **No effects**: sandboxes (`HOME`, `MCLI_DIR`, working directory) stay empty,
  an existing `mcli.db` stays byte-identical, the loopback relays' stores are
  unchanged (no EVENT published), and rejected invocations make zero relay
  connections.

### Windows (blocked at the time; later covered by CI)

Update 2026-10-29: the full suite passed on a GitHub-hosted native Windows runner; see
[the CI result recorded with slice 3](#ci-result-for-the-previous-push-3a4145e-native-windows-passed).
The text below is the original record.

Not run: row-by-row Windows results, the watchdog test's Windows behavior after
the 6 s change, and the new `pilot-windows` CI job. Until they exist, SC-005 and the
Windows half of every row above are unverified, not passed.

T001-T052 are recorded; the Windows half stays open. T053 (final gates) is next.

## T053 Final Gates: 2026-10-28

Native Linux (`x86_64`), development toolchain rustc 1.97.0 (pinned by
`rust-toolchain.toml`), declared minimum 1.94.0 installed separately
(rustc 1.94.0 `4a4ef493e`), on `develop` after `4feddba`. No lockfile, toolchain or
minimum was changed.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --no-fail-fast` | PASS, 428 tests: lib 124, cli_functions 67, integration_tests 33, order_list_process 46 (15.2 s), parser_disputes 9, parser_dms 19, parser_orders 110, util_misc 17, doctests 3; 0 failed, 0 ignored |
| `cargo build --locked` | PASS |
| `cargo +1.94.0 check --locked --all-targets --all-features` | PASS |

No pre-existing failure and no skipped gate on Linux. Not covered by this task:
Windows runs, release builds and `cargo doc`, the MCP repository's own gates.

T001-T053 are recorded. T054 (requirement reconciliation) is next.

## T054 Requirement Reconciliation: 2026-10-28

Documentation only. The table is in
[tests/README.md](README.md#order-list-pilot-requirement-evidence) and is the single
place that maps FR-001 to FR-009 and SC-001 to SC-005 to tests and status; it
links the contracts and the generated fixtures. Also refreshed there: the stale
per-file test inventory (counts from the T053 run). Earlier evidence sections in
this file are unchanged.

Result: FR-001 to FR-009 and SC-001 to SC-004 are **verified on Linux with loopback
relays**, each with the limits stated in the table (no process-level `internal_error`,
no AUTH exercise, FR-008 prose without an automated check). **SC-005 is open**
because no native Windows run exists. No row is marked complete by documentation
alone.

T001-T054 are recorded. T055 (status updates) is next.

## T055 Status Update: 2026-10-28

Documentation only, **left uncommitted** as the task requires. Status lines now say
what T054 proves and no more: `plan.md` (status, evidence links, task-list note),
`checklists/requirements.md` (a pointer to the reconciliation; no item changed),
the cross-repository roadmap (a "Progress" section: Phase 1 baseline and Phase 2
done on Linux; Phases 3-6 and 8 not started; Phase 7 partly done; critical-path
step 1 done), and, because they would otherwise state the opposite of the
evidence, the status lines of `spec.md`, `data-model.md`, `contracts/cli.md`
and `quickstart.md`, `AGENTS.md` and the handoff section of `docs/ai-development.md`.

Retained blockers: the native Windows run and the first CI Windows job run (SC-005),
no process-level `internal_error`, NIP-42 AUTH never exercised, live relays and
nodes untested, the write commands and the MCP unmigrated. No next feature was
started and nothing was staged, committed or published for this task.

All 55 tasks are checked; the pilot is not accepted for release until the Windows
result exists.




## Order Info, Slice 1: Characterization And Fixtures: 2026-10-28

Native Linux, rustc 1.97.0, on `develop` after `264a8eb`. Tests and fixtures only; no production
change. Lean mode: this is the only evidence entry for the slice (see
[tasks.md](../specs/002-order-info-json/tasks.md)).

- **Legacy `ordersinfo` pinned** (`cli_functions::ordersinfo_characterization`, removed with the command in slice 4):
  repeated ids through `-o` or `--orderids`, any letter case; duplicates are kept and no id is accepted at parse
  time (the handler rejects an empty list later, after the context exists); a bad UUID fails; the registry already
  names it `order.info` and human-only. Already known from the code: it signs and receives with the identity key,
  creates the identity on first use and waits for the first message from the node.
- **Wire facts** (`parser_orders::info_contract`, 3 tests, pinned `mostro-core` 0.17.3): the `orders` request with
  `Ids` keeps its request id and proves the signing identity; the `Orders` reply keeps request id, orders and trade
  keys and has no buyer invoice; `not_found`, `too_many_requests` and `invalid_parameters` travel as `CantDo` with
  snake-case reasons. No divergence found.
- **Shared harness**: the process harness moved from `tests/order_list_process.rs` to `tests/common/harness.rs`
  (`Sandbox`, `LoopbackRelay`, `Backend`, `Captured`, `run_with_watchdog`, `set_node_env`, `assert_no_effects`,
  `closed_pipe`, `run_with_node_env`, `SENTINEL`). The 46 existing process tests pass unchanged.
- **Scripted fake daemon and identity seeding** (`tests/common/fake_mostro.rs`, self-tested in
  `tests/order_info_process.rs`, 3 tests): the daemon decrypts and records requests (author, identity, request id,
  action, ids) and replies by script (orders, refusals, malformed reply, noise, wrong signer or recipient, delay,
  silence); `info_event` builds the kind-38385 event with any subset of its tags; `seed_identity` creates a real
  `mcli.db` through the production `User` code. Mutation: changing the reply's request id failed two of the three
  tests; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (437 tests: lib 124, cli_functions 70,
  integration 33, order_info_process 3, order_list_process 46, parser_orders 113, others unchanged, doctests 3),
  `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass. Windows not run.

Nothing here claims any `order info` behavior: the command does not exist yet.


## Order Info, Slice 2: Identity And Outcome Pieces: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `1d0bf47`. No command exists yet; this slice adds the
pieces it needs. Lean mode: this is the only evidence entry for the slice.

- **Windows CI finding (before the slice).** The first native Windows run (CI job `Pilot tests (native
  Windows)`) failed three `ci_wiring` tests: a Windows checkout converts line endings and the test parsed
  `\n`. Fixed in `1d0bf47` (tests normalize `\r\n`; checked locally by converting both workflow files to CRLF);
  the Windows job now uses `--no-fail-fast`. Every other test binary that ran there passed: lib 123,
  `cli_functions` 69, and 30 of 33 `integration_tests`. Because `cargo test` stopped at the failing binary, the
  Windows process suite has not run yet.
- **`src/identity.rs`** (8 tests): read-only loader; `db.rs` now delegates its key derivation to it. A missing
  directory or file, or an empty `users` table, is `Missing`; garbage, no `users` table, a NULL or invalid mnemonic
  is `Unreadable`; the valid store yields the production key. Nothing is created and every file in the directory is
  unchanged (name, size, hash). Finding while testing: a read-only open of a **WAL-mode** store makes SQLite create
  `mcli.db-wal` and `mcli.db-shm`; the loader opens such a store as `immutable` when no sidecars exist (the mnemonic
  never changes), and a store in use by a writer is read without adding or removing a file. The CLI's own databases
  are rollback-journal mode, so this guards stores from other builds.
- **`src/outcome/codes.rs`, `outcome.rs`**: eight new error codes, two warnings, `ErrorCode::PILOT` and
  `WarningCode::PILOT` (frozen), optional `error.reason` (a snake-case word, otherwise dropped; omitted from the derived
  schema), `ErrorInfo` no longer `Copy`, `Envelope::refused`, `ExecutionMeta::request`. The code types moved out of
  `outcome.rs` (394 of 400 lines) into `codes.rs`; now 205 and 205.
- **`src/outcome/info.rs`** (10 tests in `parser_orders::info_contract`): `OrderInfoInput` (distinct ids in order, at
  least one, no cap) and `OrderInfoData::from_reply`: request order, each id once, unrequested and repeated orders
  dropped, every id returned or not returned exactly once (all 64 subsets of six ids), the empty reply as the
  not-found success, one warning per kind, canonical decimal strings, trade keys as lowercase hex or null, times as
  UTC text plus the exact integer, the buyer invoice never present.
- **`src/outcome/schema.rs`**: `OutcomeRules::{read, request}`; shared `$defs` for the code and warning enums list the
  frozen pilot set plus the command's own; the `reason` property exists only for commands that declare it; request
  stage combinations are checked per branch. The checked-in `docs/schemas/*.json` still equal the generated output
  (`schema_fixtures` pass), so the pilot documents did not change. New tests conform nine real request outcomes and
  reject seventeen contradictory ones.
- **`src/cli/registry.rs`**: `JsonSupport::Request` and its pre-execution stages (not attempted, node unknown, no local write).
- **Checks on the checks**: widening `is_reason` failed the reason test; dropping the ambiguous-publication stage
  combination failed the conformance test; removing the `immutable` choice failed the WAL test; the shared `$defs`
  failure was seen before the generator fix. All restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (459 tests: lib 141, `cli_functions` 70,
  `integration_tests` 33, `order_info_process` 3, `order_list_process` 46, `parser_orders` 123, others unchanged,
  doctests 3), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.
  Native Windows not rerun.


## Order Info, Slice 3: The Exchange: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `3a4145e`. The exchange exists and is tested against the
scripted daemon; no command uses it yet. Lean mode: this is the only evidence entry for the slice.

- **`msg::exchange`** (public) with the private doors `send_msg.rs` (wrap and publish; a failure counts as an explicit
  rejection only if every relay's text carries a machine-readable prefix such as `blocked:`, otherwise it is ambiguous),
  `reply.rs` (merged relay streams armed before subscribing; the existing correlation rule decides; a relay that closes
  the subscription or goes away only fails the wait when every relay has), `node_info.rs` (best-effort kind-38385 read:
  protocol version and expected work). Types: `ExchangeLimits`, `Outbound`, `Expect`, `Exchanged`, `ExchangeFailure`
  (code, publication and node-acceptance stages, warnings). The client stays private; five `compile_fail` doctests prove
  `Session`, `orders`, `send_msg`, `reply` and `node_info` cannot be imported.
- **Contract tests** (12 new, `tests/order_info_process.rs`, against the fake daemon on a loopback relay that also counts
  request EVENTs): one signed request reaches the node and the relay once; 20 instant replies in a row are never missed;
  an other-request reply, a wrong action, a wrong signer, a reply addressed to someone else and a duplicate never decide
  the result; traffic that is not the answer ends in a timeout with one request and no retry; refusals and odd payloads
  are returned to the caller; silence is `reply_timeout` (published, node unknown) within budget with bounded cleanup
  and our connection gone; protocol 1 and too little configured work fail before anything is published (and an explicit
  transport choice or enough work proceeds); a missing info event is not an error; advertised first-contact work explains
  a silent node as `pow_required` (published); a relay rejecting writes gives `request_rejected` (not attempted); an
  unreachable relay gives `relay_connection_failed` and a relay answering every REQ with CLOSED gives `relay_read_failed`
  (published); a request without an identifier fails before connecting.
- **Checks on the checks**: disabling correlation, the early work check or the arm-before-publish order failed tests;
  publishing the request twice was **not** caught at first because the relay deduplicates the same event, so the relay now
  counts EVENT messages before deduplication and the mutation fails two tests. All restored.
- **Boundary tests** updated deliberately: `.send_event(` gains `msg/send_msg.rs`, `.fetch_events(` gains
  `msg/node_info.rs`, `.subscribe(` gains `msg/exchange.rs`; no legacy count changed. `util::events::read_info_tag_from_event`
  became `pub(crate)` for reuse.
- Gates: fmt, Clippy `-D warnings` (two findings fixed: a large enum variant, an unneeded copy), `cargo test --locked
  --no-fail-fast` (484 tests: lib 146, `cli_functions` 70, `integration_tests` 33, `order_info_process` 15,
  `order_list_process` 46, `parser_orders` 123, others unchanged, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

### CI result for the previous push (`3a4145e`): native Windows passed

The GitHub run `40934312083` was green in every job, including `Pilot tests (native Windows)` on `windows-latest`
(MSVC), which ran every test binary with `--no-fail-fast`: lib 140, `cli_functions` 69, `integration_tests` 33,
`order_info_process` 3, `order_list_process` 46 (32 s), `parser_disputes` 9, `parser_dms` 19, `parser_orders` 123,
`util_misc` 15 and 3 doctests, none failed. The counts are one below Linux in lib and `cli_functions`, and two below in
`util_misc`, which are Unix-only tests. This is the first Windows evidence for the order-list pilot (its commands, process
tests with planted secret sentinels, streams and exit codes); it is a hosted runner, not a developer workstation. The
slice-3 changes have not run on Windows yet.


## Order Info, Slice 4: The Command: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `3919d99`. `mostro-cli order info --order UUID...` exists; the
legacy `ordersinfo` (command, handler `cli/orders_info.rs`, characterization tests) is removed with no alias. Lean mode:
this is the only evidence entry for the slice.

- **Extraction first** (behavior-preserving, tests rerun): the dispatch of migrated commands and its tests moved from
  `cli.rs` to `cli/migrated.rs` (`cli.rs` 316 to 189 lines; `migrated.rs` 290 with the third command).
- **Pieces**: `OrderCommands::Info` (`--order`, required, repeatable, no cap), the registry entry `order.info` moved to
  the path `order info` with `JsonSupport::Request`, `commands/order/info.rs` (the operation: one `exchange`, mapping
  `Orders`, `CantDo(not_found)` to the empty success, other `CantDo` to `node_refused` with its reason, anything else to
  `reply_malformed`), `parse_human/order_info.rs` (pure, the familiar labelled lines, nothing invented for values the
  node omitted, control characters escaped), `--pow`/`--transport` handling (`POW`/`TRANSPORT` as fallbacks; only `nip44`;
  an explicit transport skips the protocol check), identity read-only after configuration, one `--verbose` line of codes and
  counts. Local failures (arguments, configuration, work, transport, identity) happen before anything is published.
- **Forced by the registry move, done here instead of slice 5**: `command_schemas` and `required_configuration` for
  `order.info`, the regenerated `docs/schemas/discovery-v1.json` (only the `order.info` descriptor changed; every other
  descriptor is identical, and `order-list-v1.json` is unchanged), the new `docs/schemas/order-info-v1.json`, and a drift
  test for it. `README.md` and `docs/commands.md` no longer describe `ordersinfo`.
- **Binary scenarios** (13 new, `tests/order_info_process.rs`, the real binary against the scripted daemon, a seeded identity
  store and a relay that counts request EVENTs): one id returns the order as one document with exactly one request, the
  identity as signer and the identity store byte-identical with nothing else created; a subset in request order with
  unrequested and repeated orders dropped and one warning; a repeated id in any case requested once; `not_found` as the
  same success; noise (other request, wrong signer, other recipient) never deciding; a refusal as exit 1 with its reason in
  JSON and one fixed human line; `reply_malformed`; a silent node as `reply_timeout` after the production 15 s budget with
  one request; human and JSON parity with a range, a negative premium and an escape sequence in the payment method (shown
  escaped, no raw control character), three runs and three requests; the output flag at six positions returning the same
  document; 300 ids in one request; `--verbose` printing one allow-listed line with no mnemonic word, id or relay address;
  an unwritable stdout (JSON and human) exiting 1 with one fixed line.
- **Grammar tests** (4 new, `cli_functions::order_info_grammar`): repeatable any-case ids with no cap; a missing, malformed or
  positional id, `-o`, `--orderids` and `ordersinfo` all rejected; the registry entry; argument errors with JSON intent keep the
  resolved command `order.info`.
- **Real binary check**: `order info --help`; `ordersinfo` is an unrecognized subcommand; with no configuration the JSON
  outcome is `configuration_missing`, with configuration but no identity `identity_missing`, nothing created.
- **Checks on the checks**: removing deduplication, mapping `not_found` to an error and dropping terminal escaping each
  failed exactly the test that guards it; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (505 tests: lib 146, `cli_functions` 71,
  `integration_tests` 34, `order_info_process` 28, `order_list_process` 46, `parser_orders` 123, others unchanged, doctests 6),
  `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass. Not yet run on Windows.

## Order Info, Slice 5: Failure Matrix And Examples: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `a8c7db3`. Lean mode: the only evidence entry for the slice. The schemas and
fixtures of this slice were done in slice 4.

- **Failure matrix** (6 new binary tests, `tests/order_info_process.rs`; each failure runs as JSON, human and `--verbose`,
  with a planted sentinel where an input carries text): no store, an empty `users` table (`identity_missing`); a file that is
  not a database, no `users` table, a null mnemonic (`identity_unreadable`); missing node key or relays
  (`configuration_missing`); a bad node key, relay, `--pow` or `--transport` (`configuration_invalid`); protocol 1
  (`protocol_unsupported`); more work expected than configured (`pow_required`); a relay rejecting writes
  (`request_rejected`); an unreachable relay (`relay_connection_failed`). For each: the exit code, `command` `order.info`,
  stages (`not_attempted`/`unknown`, `persistence` `not_applicable`), JSON on stdout only and nothing on stderr, human on
  stderr only as one `error:` line, no sentinel and no `127.0.0.1` in any stream, zero requests at the relay, and the data
  directory byte-identical with nothing created elsewhere. An explicit `--transport nip44` with enough `--pow` succeeds on a
  node advertising protocol 1.
- **Argument errors**: exit 2, `invalid_arguments`, `order.info` as the command for a missing, malformed or valueless
  `--order` and an unknown option, `null` for the removed `ordersinfo`; no request, nothing created, the bad value not echoed.
- **Unwritable stderr** (JSON mode) keeps exit 1 and the document.
- **Examples** (`contracts/examples.json`, 3 tests in `schema_fixtures`): every proposed outcome conforms to the checked-in
  schema, is reproduced by the owned constructors, and invalid variants are rejected.
- **Checks on the checks**: mapping an unreadable store to `identity_missing` and skipping the work check each failed the
  matrix; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (508 tests: lib 146, `cli_functions` 71,
  `integration_tests` 37, `order_info_process` 34, `order_list_process` 46, `parser_orders` 123, `parser_dms` 19,
  `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets
  --all-features`: all pass. Slice 4 and 5 are not yet run on Windows until CI runs after a push.

## Order Info, Slice 6: Docs And Verification: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `d593399` (pushed). Documentation only; no production change. Lean mode: the
only evidence entry for the slice.

- **Docs**: `docs/architecture.md` (new identity, request-and-reply and operation rows, the order-info effects rule, the
  `orders_info.rs` removal, a note that the size table is a pilot-time snapshot), the `README.md` and `docs/commands.md`
  migration notes (done in slice 4), the requirement evidence in `tests/README.md`, and the status lines of the spec, plan,
  quickstart, `AGENTS.md`, `docs/ai-development.md` and the roadmap (Amendment 10). The frozen boundary counts had been
  updated deliberately in slices 3-4 and are green.
- **Offline smoke check** (the real binary, empty environment, sandboxed home, closed stdin): `order info --order ... --output
  json` gives `configuration_missing`, exit 1, `not_attempted`, nothing created; `schema --output json` lists `order.info`.
- Gates on the final tree: fmt, Clippy `-D warnings`, `cargo test --locked` (508 tests), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features` (the last three as in slice 5; no source changed since).
- **Open**: the regtest probe (separate authorization, regtest identities only, never `devenv.sh clean`), Windows evidence
  for slices 4-6 via CI, and the MCP `get_order` migration.

## Order New, Slice 1: Characterization And Fixtures: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `2cec5e7` (pushed). No production change. Lean mode: the only evidence entry
for the slice.

- **Legacy `neworder` pinned** (`cli_functions::neworder_characterization`, removed with the command in slice 4): required
  flags and defaults (amount 0, premium 0, expiration 0); long flags are squashed (`--fiatcode`, `--fiatamount`,
  `--paymentmethod`, `--expirationdays`) and short ones exist; the kind text is not checked by the grammar; the `100-200`
  range syntax and its failures (`200-100`, `100-100`, `1-2-3`, `100-`, `-100`, text, empty), zero accepted; the
  confirmation (blank, `y`, trailing spaces accept, leading space, `n`, other text and EOF do not); the Yadio check.
- **Wire facts** (`tests/order_new_process.rs`): the request is authored and signed by the trade key under the identity seal;
  the inner signature verifies; the index rides in the message, the id is the node's; the payload of a fixed and of a range
  order (`fiat_amount` 0 with `min`/`max`).
- **Legacy local effects pinned**: computing the next key writes nothing; `save_order` writes the order row and the index
  together, only when called after an accepted reply; saving the same order again replaces its row and advances the index; a
  store that cannot be written fails the save and leaves the local index behind the node's (the `invalid_trade_index` trap
  the new command avoids).
- **Fixtures**: the scripted daemon records the trade index, the order and whether the inner signature verifies, and
  `new_order_daemon(last, bonded)` applies the source's checks in its order (signature, index above the last or an identity used
  as trade key, half-specified range, zero fiat amount) and remembers the index only after every check passes; replies go to
  the trade key; `order_created` and `bond_requested` build the accepted and bonded answers; `set_last_trade_index` edits the
  seeded store. Self-tests cover an acknowledgement and its trade-key addressing, each refusal, the index kept on refusal and
  spent on success, the bonded answer and the seeded index.
- **Check on the check**: letting the scripted node remember the index before its checks failed the refusal self-test; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (520 tests: lib 146, `cli_functions` 76,
  `integration_tests` 37, `order_info_process` 34, `order_list_process` 46, `order_new_process` 7, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass. CI result for slice 6 of order info (`edeb118`)
  not yet read.

## Order New, Slice 2: Exchange And Store: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `2082c1a` (pushed). Lean mode: the only evidence entry for the slice.

- **Exchange**: `Outbound` gains `identity` (the seal signer) next to `signer` (author, inner signer and the key replies are
  addressed to); `order info` passes its identity for both and its 34 process tests are unchanged. Three new tests (red first:
  the field did not exist): a request authored by a trade key under an identity seal is acknowledged and the reply addressed to
  the trade key is heard; a bond request and a refusal are returned for the caller to judge when the caller allows those actions;
  an acknowledgement addressed to the identity instead of the trade key is never heard (`reply_timeout`, published).
- **Store** (`src/store.rs`, 5 tests in `tests/order_new_process.rs`): opens the existing `mcli.db` for writing, never creates it,
  one operation per open so no connection or sidecar file is left behind. `next_trade` (recorded index plus one, same derivation
  as the rest of the program, writes nothing; the directory is byte-identical afterwards; `Debug` shows no key), `advance` (moves
  forward only), `save_order` (order row and index in one transaction; with no user row nothing is written and the error is
  `Missing`). A missing store, a file that is not a database and a read-only file are reported as `Missing`/`Unwritable` with
  nothing created and the garbled file unchanged.
- **Checks on the checks**: removing the user-row check in `save_order`, letting `advance` move backwards (this one first
  survived because the test only looked at the final value; the test now checks after the older index) and sealing with the
  signer instead of the identity each failed exactly the test that guards it; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (528 tests: lib 146, `cli_functions` 76,
  `integration_tests` 37, `order_info_process` 34, `order_list_process` 46, `order_new_process` 15, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order New, Slice 3: Outcomes: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `94a2a94` (pushed). Lean mode: the only evidence entry for the slice. The
registry move, discovery and the generated `order-new-v1.json` need the grammar and land in slice 4 (as for order info).

- **Extraction first** (behavior-preserving): the stage and rules types of `outcome/schema.rs` moved to `outcome/rules.rs`
  (the file was 355 of 400 production lines); the pilot and info schemas and fixtures are unchanged.
- **Envelope**: `Envelope::uncertain` (the only constructor of the status, never exit 0, data only when the node answered),
  `ExecutionMeta::write` (publication, acceptance, persistence), `OutcomeRules::write` with `Stages::Write` and per-status stage
  combinations (success `published/accepted/applied`; failures, uncertain outcomes and their stages as in the data model); a
  command with no uncertain codes still generates two statuses and three branches.
- **Codes**: `order_declined`, `bond_required`, `order_not_saved`, `store_unwritable` (exit 1, fixed prose, scoped to this
  command; the pilot sets are still frozen).
- **Input and data** (`outcome/new.rs`): `OrderNewInput::validate` (currency 3-6 alphanumerics uppercased, positive fiat amount, a
  range needs a larger maximum and a zero sats amount, payment method and invoice bounded without control characters, premium
  -100..=100, expiration 0..=365 days) naming only the option, never its value; `small_order(now)` builds the legacy shape (pending,
  no id, fixed amount or `min`/`max` with fiat amount 0, absolute expiry); `OrderNewData::from_created` maps the node's order to
  explicit fields with the trade index and public key, and needs an id.
- **Schema tests** (7 unit tests): every kind of outcome conforms (ok, each local failure, proven rejection, refusal with reason,
  bond with the index recorded or not, `order_not_saved` with data, timeout, malformed, lost wait and ambiguous publication as
  uncertain, unresolved command); contradictions do not (wrong stages for a status, codes under the wrong status, request-only
  stages, malformed index and key, `uncertain` without the schema allowing it).
- **Checks on the checks**: letting uncertain outcomes use the failure codes, dropping one allowed uncertain stage combination,
  dropping the sats rule for ranges and allowing an unknown acceptance on success each failed the test that guards it; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (536 tests: lib 154, `cli_functions` 76,
  `integration_tests` 37, `order_info_process` 34, `order_list_process` 46, `order_new_process` 15, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order New, Slice 4a: The Command: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `559d2f0` (pushed). `mostro-cli order new` exists; the legacy `neworder` (command,
handler `cli/new_order.rs`, the Yadio call, the preflight and characterization tests) is removed with no alias. Lean mode: this
and the 4b entry are the evidence for the slice; 4b adds the binary scenarios.

- **Pieces**: `OrderCommands::New(NewArgs)` (`--side --currency --fiat-amount [--fiat-amount-max] --payment-method [--sats]
  [--premium] [--invoice] [--expiration-days] [--yes]`), registry `order.new` at `order new` with the new `JsonSupport::Write`
  (rejected-before-run stages `not_attempted/unknown/not_attempted`), `commands/order/new.rs` (the operation), `cli/order_new.rs`
  (validation, then configuration, identity and store checks in order of cost, preview and prompt in human mode only, JSON without
  `--yes` refused with `invalid_arguments` and nothing done, `--verbose` printing one code-only line), `parse_human/order_new.rs`
  (preview and result), `Store::ensure_writable`, the generated schema and `docs/schemas/order-new-v1.json`, regenerated
  `discovery-v1.json` (only the `order.new` descriptor changed).
- **Operation tests** (7 in `tests/order_new_process.rs`, the real `order_new` against the scripted daemon and a seeded store):
  an accepted order is returned and saved with its index; a refusal is `node_refused` with its reason and writes nothing; silence
  and an unusable reply are `uncertain`, never retried, with the index spent and no order; a bonded node is `bond_required` with
  the index spent; an order that cannot be saved is `uncertain` (`order_not_saved`) with what the node made, both when the whole
  store is gone and when only the orders table is (the index is still spent); needed work stops the command before anything is
  sent or written and leaves `mcli.db` byte-identical.
- **Grammar tests** (5, `cli_functions::order_new_grammar`): required options and defaults, every option with a negative premium,
  the side checked and text amounts rejected, every legacy spelling gone, the registry entry. The registry, discovery and
  boundary tests were updated to the new fact (four machine commands).
- **Real binary check**: `order new --help`; `neworder` is an unrecognized subcommand; JSON without `--yes` is `invalid_arguments`
  exit 2; with `--yes` and no configuration `configuration_missing` exit 1; nothing created.
- **Checks on the checks**: not spending the index after silence, spending it on a refusal, treating an unsent request as uncertain
  and skipping the index spend after a failed save each failed exactly the test that guards it (the last one first survived because
  the failure test also broke the index write; a second test now drops only the orders table); restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (533 tests: lib 155, `cli_functions` 64,
  `integration_tests` 38, `order_info_process` 34, `order_list_process` 46, `order_new_process` 22, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6; fewer than before because the legacy preflight and
  characterization tests went with the command), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets
  --all-features`: all pass.

## Order New, Slice 4b: Binary Scenarios: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `ba8ae91` (pushed). The real binary against the scripted daemon, a seeded identity
store and a sandboxed home; stdin is a closed descriptor or a file the test wrote, never a pipe it has to feed. Lean mode: with 4a,
the evidence for the slice.

- **Success in JSON** (`--yes`): exit 0, one document that conforms to the checked-in `order-new-v1.json`, nothing on stderr,
  stages `published/accepted/applied`, one request at the daemon, the order row and index 1 in the store, the id in the row equal
  to the id shown; neither the trade private key (read from the store), the mnemonic nor a relay address appears in any stream.
- **Confirmation**: JSON without `--yes` is `invalid_arguments` exit 2 with the document conforming, no request and the data
  directory byte-identical. Human with `--yes` prints the created order and no preview or prompt. Through the prompt `y`, blank
  and `Y` + CRLF accept (preview on stdout, the question on stderr); `n`, `no`, `maybe`, an empty input and a closed stdin decline
  with exit 1, the fixed `order_declined` line, no request and an untouched store.
- **Refusal** (node's index ahead): JSON `node_refused` with reason `invalid_trade_index`, data null; human one fixed line with the
  reason; the store byte-identical; one request per run.
- **Silence**: after the production 15 s budget `uncertain` / `reply_timeout`, `published/unknown/applied`, nothing on stderr,
  exactly one request, index 1 spent and no order row.
- **Bonded node**: `bond_required` as an error with the index spent. **Unsaved order** (orders table dropped): JSON `uncertain` /
  `order_not_saved` with the order the node made and persistence `failed`; human prints the order and the error plus the line
  telling the caller to check `order list`; each attempt spent its own index. **Range order**: `min`/`max` reach the node with fiat
  amount 0, and the row and document carry both bounds. **`--verbose`**: exactly `verbose: order.new ok`.
- **Checks on the checks**: dropping the `--yes` rule for JSON, accepting end of input at the prompt and skipping the prompt each
  failed the test that guards it; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (543 tests: lib 155, `cli_functions` 64,
  `integration_tests` 38, `order_info_process` 34, `order_list_process` 46, `order_new_process` 32, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.
- Still open for slice 5: the failure matrix at binary level (identity, configuration, protocol and work, rejected relay, ambiguous
  publication, unwritable store, many bad inputs, secret sentinels, unwritable streams), `contracts/examples.json` and its validation.

## Order New, Slice 5: Failure Matrix And Examples: 2026-10-29

Native Linux, rustc 1.97.0, on `develop` after `b90c145` (pushed). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (binary, `tests/order_new_process.rs`; each case runs as JSON, human and `--verbose`, all with `--yes`, a planted
  sentinel where an input carries text): no store and an empty `users` table (`identity_missing`); a file that is not a database, no
  `users` table and a null mnemonic (`identity_unreadable`); missing node key or relays (`configuration_missing`); a bad node key,
  relay, `--pow` or `--transport` (`configuration_invalid`); protocol 1 (`protocol_unsupported`); more work expected than configured
  (`pow_required`); a relay rejecting every write (`request_rejected`, index unspent); an unreachable relay
  (`relay_connection_failed`); a read-only store file (`store_unwritable`). For each: exit 1, `order.new`, status `error`,
  stages `not_attempted/unknown/not_attempted`, a document that conforms to `order-new-v1.json`, nothing on stderr in JSON mode and
  one `error:` line on stderr only in the others, no sentinel and no relay address in any stream, no request at the daemon, the data
  directory byte-identical and nothing created elsewhere. An explicit `--transport nip44` with enough `--pow` succeeds on a node
  advertising protocol 1.
- **A defect found and repaired**: the first run of the read-only case showed the command *published* the order and then failed to save
  it (`uncertain`): SQLite silently opens a file it may not write read-only, so `BEGIN IMMEDIATE` passed. `Store::ensure_writable` now
  also requires the file to accept a write; the store test compares the check with an actual open for writing. (The check is skipped
  when the process may write a read-only file, as root.)
- **Argument errors**: twelve cases exit 2 with `invalid_arguments`, nothing sent or written, nothing echoed, nothing on stderr: bad
  currency, zero fiat amount, a payment method with a control character, a range maximum not above the minimum, a range with sats,
  premium and expiration out of range, an invoice with whitespace, a bad side, a text amount, missing required options (all keeping the
  resolved command `order.new`) and the removed `neworder` (command `null`).
- **Unwritable streams**: a closed stdout after the order was created exits 1 with one fixed line per mode (the order was made and saved
  all the same, one per run); a closed stderr changes neither the exit nor the JSON document.
- **Examples** (`contracts/examples.json`, ten outcomes: created, range created, refusal, bond, timeout and unsaved order as
  `uncertain`, declined, identity missing, `--yes` required, removed spelling): each conforms to the checked-in schema, is reproduced by the
  owned constructors, carries data only when it should, and 18 invalid variants do not conform.
- **Checks on the checks**: the missing writability check (above), mapping an unreadable store to missing, and dropping the currency
  character rule each failed a test; the last one first survived because the case used a two-letter currency that the length rule
  also rejects, so the case now uses `U$D`; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (554 tests: lib 155, `cli_functions` 64,
  `integration_tests` 41, `order_info_process` 34, `order_list_process` 46, `order_new_process` 40, `parser_orders` 123,
  `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order New, Slice 6: Docs And Verification: 2026-10-30

Native Linux, rustc 1.97.0, on `develop` after `24c2dd2` (pushed). CI is green for every `order new` push up to slice 5, including
the Windows job. Documentation only; no production change. Lean mode: the only evidence entry for the slice.

- **Docs**: `docs/architecture.md` (the order-new flow replaces the legacy one, the `store.rs`, `commands/order/new.rs`,
  `cli/order_new.rs` and `outcome/new.rs` entries, the removed `new_order.rs`), `docs/pow_error_handling.md` (the manual PoW check
  now uses `order new` and `pow_required`), the requirement evidence in `tests/README.md`, `specs/003-order-new-json/quickstart.md`
  (focused commands, offline smoke check, acceptance matrix, the regtest probe procedure), and the status lines of the spec, plan,
  `AGENTS.md`, `docs/ai-development.md` and the roadmap (Amendment 11). The README and command reference were migrated in slice 4a; the
  boundary counts and CI wiring needed no change (the transport owners are still `msg`).
- **Offline smoke check** (the real binary, empty environment, sandboxed home, closed stdin): `order new` without `--yes` in JSON is
  `invalid_arguments` exit 2; with `--yes` and no configuration `configuration_missing` exit 1; `neworder` is an unrecognized
  subcommand; nothing is created.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked` (554 tests), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass (no source changed since slice 5).
- **Open**: the regtest probe (separate authorization, regtest identities only, never `devenv.sh clean`), MCP `create_order`, and
  `order take`.

## Order Take, Slice 1: Characterization And Fixtures: 2026-10-30

Native Linux, rustc 1.97.0, on `develop` after `4be1332` (pushed). No production change. Lean mode: the only evidence entry for the
slice.

- **Legacy `takesell`/`takebuy` pinned**, removed with the commands in slice 4:
  - **Payload bytes** (4 unit tests in `cli/take_order.rs`): take-buy sends `Amount(n)` only when an amount is given and ignores an
    invoice; take-sell without an invoice sends `Amount(n)` and `Amount(0)` when nothing is given; with an invoice it sends the payment request
    with the amount inside it, and an **invalid invoice is printed and sent anyway**; only the two take actions build a payload.
  - **Grammar** (4 tests, `cli_functions::take_characterization`): the order id is required and a UUID in any case; the long flag is
    squashed (`--orderid`); `-i`/`--invoice` exists only on take-sell; the amount is a non-negative integer, zero parses; both are registered
    legacy commands with canonical names `order.take-sell`/`order.take-buy` and no JSON.
  - **Local effects** were already pinned by the existing unit tests of `taken_sell_order` and `save_taken_sell_order` (row from the book
    or minimal, trade keys and index saved, an existing row left alone).
- **Fixtures** (`tests/common/fake_mostro.rs`, `tests/order_take_process.rs`): the scripted daemon records the order id, amount and invoice a
  take carries; `take_daemon(book, bonded)` applies the source's checks in order (index and signature, order known, a pending order for the
  taker, side, status, own order, range amount, invoice) and keeps a book (statuses, the taker set, the index, remembered only after every
  check passed). Replies as in the source: `add-invoice` with the order (take-sell, no invoice); `waiting-seller-to-pay` with no payload to the
  taker while the hold invoice `pay-invoice` is addressed to the **maker** (take-sell, invoice); `pay-invoice` with the order and hold invoice
  (take-buy); `pay-bond-invoice` with the order parked (bonded node); `cant-do` otherwise.
- **Self-tests** (6): each reply and its addressing (the maker's own listener hears the hold invoice), a range order inside and outside its
  bounds with the amount echoed, every refusal (stale index, unknown order, wrong side both ways, taken order, invalid invoice, own order,
  pending order) leaving the index and the taker set untouched, and the bonded answer.
- **Checks on the checks**: remembering the index before the checks, addressing the hold invoice to the taker, and dropping the pending-order
  check each failed a self-test; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (568 tests: lib 159, `cli_functions` 68, `integration_tests` 41,
  `order_info_process` 34, `order_list_process` 46, `order_new_process` 40, `order_take_process` 6, `parser_orders` 123, `parser_dms` 19,
  `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`:
  all pass.

## Order Take, Slice 2: Shared Dispatch And Outcomes: 2026-10-30

Native Linux, rustc 1.97.0, on `develop` after `480d241` (not pushed yet). Lean mode: the only evidence entry for the slice.

- **Extraction first** (behavior-preserving): the steps every write command shares moved from `cli/order_new.rs` (215 lines) to
  `cli/write.rs` (configuration, identity, store and trade resolution; the JSON `--yes` rule; the refusal envelope; the prompt with a
  command-supplied preview and question; presentation, the verbose line and the uncertain advice). `order_new.rs` is 77 lines. The 40 `order new`
  tests, binary scenarios included, are unchanged and green. `WriteOptions` (pow, protocol check) moved to `commands/order` and `NewOptions` is an
  alias.
- **Input and data** (`outcome/take.rs`, 8 unit tests): `TakeSide` (command name, action and kind of the order taken); `OrderTakeInput::validate`
  (positive amount; an invoice only on take-sell, bounded, no whitespace or control characters; the error names the option, never the value, so an
  invalid invoice is now refused locally instead of printed and sent as the legacy command did); `payload()` is byte for byte the legacy one for every
  invoice and amount combination (the slice 1 cases); `minimal_order()` (id, side of the order, `waiting-payment`, the amount taken, empty text);
  `OrderTakeData::from_taken` maps `add-invoice`, `waiting-seller-to-pay` and `pay-invoice` to `add_invoice`, `wait_for_seller_payment` and
  `pay_hold_invoice` with the order to save, and refuses a reply that names another order or carries no id; the hold invoice is data and appears
  nowhere else.
- **Schema tests**: each canonical name generates its own schema (`order.take-sell`, `order.take-buy`, three statuses); real outcomes of every next step and
  an unsaved take conform; eight contradictions do not.
- **Checks on the checks**: dropping the other-order check, allowing an invoice on take-buy, changing the zero amount of the legacy payload and changing the
  minimal row's status each failed a test; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (577 tests: lib 168, `cli_functions` 68, `integration_tests` 41,
  `order_info_process` 34, `order_list_process` 46, `order_new_process` 40, `order_take_process` 6, `parser_orders` 123, `parser_dms` 19,
  `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order Take, Slice 3: The Operation: 2026-10-30

Native Linux, rustc 1.97.0, on `develop` after `9304528` (pushed). Lean mode: the only evidence entry for the slice.

- **Extraction first** (behavior-preserving): the outcomes every write command shares (`failed`, `malformed`, `bond_required`, `refused`, `saved`,
  `spend_index`, `unsent`) moved from `commands/order/new.rs` to `commands/order/write.rs` (`Sent`), with the index policy stated once; `new.rs` shrank
  and its 40 tests, binary scenarios included, are unchanged and green.
- **The operation** (`commands/order/take.rs`, `order_take` for both sides): one `exchange` with the take actions allowed (`add-invoice`,
  `waiting-seller-to-pay`, `pay-invoice`, `pay-bond-invoice`, `cant-do`), each reply mapped by side: take-sell accepts `add-invoice` with an order or
  `waiting-seller-to-pay` naming the order taken; take-buy accepts `pay-invoice` with the order and the hold invoice; anything else, or a reply about
  another order, is `reply_malformed` (uncertain).
- **Operation tests** (10 in `tests/order_take_process.rs`, the real `order_take` against the scripted daemon and a seeded store):
  take-sell without an invoice asks for one and saves the node's order (row `waiting-buyer-invoice`, index 1, trade key stored, the node's book index
  advanced); take-sell with an invoice waits for the seller and saves the minimal row (kind `sell`, `waiting-payment`, empty currency); take-buy returns the
  hold invoice as data and stores no invoice; a range take records the amount taken; refusals (stale index, wrong side, unknown order) carry their reason and
  write nothing; silence is `uncertain` with the index spent and no row; three replies that do not fit (another order's `add-invoice`, a hold invoice for a
  take-sell, a waiting reply naming another order) are `uncertain`/`reply_malformed`; a bonded node is `bond_required` with the index spent; an order table
  that cannot be written is `order_not_saved` with the data returned and the index still spent; needed work stops everything with `mcli.db` byte-identical.
- **Checks on the checks**: dropping the order check on the waiting reply, spending no index after a failed save, spending none after a bond request and
  accepting a hold invoice for a take-sell each failed a test (the last one first survived because the odd reply carried no order; it now carries a
  matching one so only the side check can catch it); restored.
- **Correction (2026-11-01)**: this entry claimed all gates passed, but `source_boundaries::production_imports_are_at_the_top_except_frozen_legacy` failed on a `pub use`
  left at the end of `commands/order/take.rs`; I had summed the per-binary counts without checking for failures, and CI (run 41306913823) caught it. The next slice moved the line to
  the top (4962983). Failures are now counted explicitly before a slice is reported.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (586 tests: lib 168, `cli_functions` 68, `integration_tests` 41,
  `order_info_process` 34, `order_list_process` 46, `order_new_process` 40, `order_take_process` 16, `parser_orders` 123, `parser_dms` 19,
  `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order Take, Slice 4a: The Commands: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `11b27cb` (pushed). `mostro-cli order take-sell` and `order take-buy` exist; the legacy `takesell` and
`takebuy` (commands, handler `cli/take_order.rs`, the book read after a take-sell with an invoice, their characterization and save tests) are removed with
no alias. Lean mode: this and the 4b entry are the evidence for the slice; 4b adds the binary scenarios.

- **Pieces**: `OrderCommands::TakeSell(TakeSellArgs)` and `TakeBuy(TakeBuyArgs)` (`--order UUID`, `--amount`, `--invoice` on take-sell only, `--yes`), two registry
  entries with `JsonSupport::Write` (the canonical names and no registry change), `cli/order_take.rs` (validation, then the shared `cli/write.rs` steps, preview and
  prompt in human mode only, JSON without `--yes` refused with `invalid_arguments`), `parse_human/order_take.rs` (the preview never shows the invoice the user gave;
  the hold invoice is alone on its line), the command-scoped schemas (`order.take-sell`, `order.take-buy`), `docs/schemas/order-take-sell-v1.json` and
  `order-take-buy-v1.json`, and the regenerated `discovery-v1.json` (only the two take descriptors changed; their old human-only entries became machine ones).
  The dispatch needed `#[command(name = "take-sell")]`: the group's lower-casing rule had named them `takesell`.
- **Grammar tests** (5, `cli_functions::order_take_grammar`): the options and defaults of both commands, a take-buy without an invoice option, the order
  required and text amounts rejected, every legacy spelling gone, both registered as JSON-capable writes. The registry, output-intent, discovery and drift tests
  were updated to the new fact (six machine commands; legacy samples now use `addinvoice`); a drift test covers the two new fixtures.
- **Boundary check at work**: the import-position test refused a `pub use` I had left at the end of `commands/order/take.rs`.
- **Docs**: `README.md`, `docs/commands.md` (the two takes replace the legacy entries, with the migration note and the MCP warning) and `docs/database.md` (the
  minimal row replaces the book read).
- **Real binary check**: `order take-sell` in JSON without `--yes` is `invalid_arguments` exit 2 naming `order.take-sell`; `order take-buy --yes` with no
  configuration is `configuration_missing` exit 1; `takesell` is an unrecognized subcommand; nothing is created.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (578 tests: lib 159, `cli_functions` 67, `integration_tests` 42, `order_info_process` 34,
  `order_list_process` 46, `order_new_process` 40, `order_take_process` 16, `parser_orders` 123, `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6; fewer
  than before because the legacy save and characterization tests went with the commands), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets
  --all-features`: all pass.

## Order Take, Slice 4b: Binary Scenarios: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `4962983` (pushed). The real binary against the scripted daemon, a seeded identity store and a sandboxed
home; stdin is a closed descriptor or a file the test wrote, never a pipe it has to feed. Lean mode: with 4a, the evidence for the slice.

- **Take-sell in JSON** (`--yes`): exit 0, one document that conforms to the checked-in `order-take-sell-v1.json`, nothing on stderr, `next_step` `add_invoice`, no
  hold invoice, stages `published/accepted/applied`, one request, the row and index 1 in the store; neither the trade private key (read from the store), the mnemonic
  nor a relay address appears in any stream.
- **The user's invoice never comes back**: a take-sell with an invoice carrying a sentinel, in JSON with `--verbose` and in human mode through the prompt, shows no
  trace of it in stdout or stderr (the preview only says "Invoice: provided"); the node received it; the store keeps no invoice; the reply is
  `wait_for_seller_payment` with a null order and `--verbose` prints exactly `verbose: order.take-sell ok`.
- **Take-buy**: JSON `pay_hold_invoice` with the hold invoice in `data.payment_request` (conforming to `order-take-buy-v1.json`); in human mode the hold invoice is alone on its
  line of stdout and never on stderr; `--verbose` prints exactly `verbose: order.take-buy ok`.
- **Confirmation**: JSON without `--yes` is `invalid_arguments` exit 2 naming the command, for both sides, with no request and the data directory byte-identical. Through the prompt
  `y`, blank and `Y` + CRLF accept; `n`, `no`, `maybe`, an empty input and a closed stdin decline with exit 1, the fixed `order_declined` line, no request and an untouched store.
- **Refusal** (node's index ahead): JSON `node_refused` with the reason and null data, one fixed human line with the reason, the store byte-identical, one request per run.
  **Silence**: after the production 15 s budget `uncertain` / `reply_timeout`, nothing on stderr, exactly one request, the index spent and no row. **Bonded node**: `bond_required`, index spent.
- **Unsaved take** (orders table dropped): JSON `uncertain` / `order_not_saved` with the hold invoice still in the data and persistence `failed`; human still prints the hold invoice on its line
  and the error plus the line pointing to `order info --order <id>`, without repeating the hold invoice on stderr. **Range take**: the amount reaches the node, the document and the row.
- **Checks on the checks**: putting the hold invoice on the same line as its label, rewording the advice line and printing the user's invoice in the preview each failed the test that guards it;
  restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (589 tests: lib 159, `cli_functions` 67, `integration_tests` 42, `order_info_process` 34, `order_list_process` 46,
  `order_new_process` 40, `order_take_process` 27, `parser_orders` 123, `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.
- Still open for slice 5: the failure matrix at binary level (identity, configuration, protocol and work, rejected and unreachable relay, unwritable store, invalid input, secret sentinels, unwritable
  streams), `contracts/examples.json` and its validation.

## Order Take, Slice 5: Failure Matrix And Examples: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `0103425` (pushed). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (binary, `tests/order_take_process.rs`; each case runs on **both** takes as JSON, human and `--verbose`, all with `--yes`, a planted sentinel where an input
  carries text): no store and an empty `users` table (`identity_missing`); a file that is not a database, no `users` table and a null mnemonic (`identity_unreadable`); missing node
  key or relays (`configuration_missing`); a bad node key, relay, `--pow` or `--transport` (`configuration_invalid`); protocol 1 (`protocol_unsupported`); more work expected than
  configured (`pow_required`); a relay rejecting every write (`request_rejected`, index unspent); an unreachable relay (`relay_connection_failed`); a read-only store file
  (`store_unwritable`). For each: exit 1, the command's own canonical name, status `error`, stages `not_attempted/unknown/not_attempted`, a document that conforms to that command's
  checked-in schema, nothing on stderr in JSON mode and one `error:` line on stderr only in the others, no sentinel, relay address or hold invoice in any stream, no request at the
  daemon, the data directory byte-identical and nothing created elsewhere. An explicit `--transport nip44` with enough `--pow` succeeds on a node advertising protocol 1.
- **Argument errors**: ten cases exit 2 with `invalid_arguments`, nothing sent or written, nothing echoed, nothing on stderr: a zero or negative amount, an invoice with whitespace or over
  4096 bytes, an invoice on take-buy, a text amount, a bad order id and a missing order (all keeping the resolved command), and the removed `takesell` and `takebuy` (command `null`).
- **Unwritable streams**: a closed stdout after the take exits 1 with one fixed line per mode and both takes are saved all the same; a closed stderr changes neither the exit nor the
  JSON document.
- **Examples** (`contracts/examples.json`, eleven outcomes across both commands: the three next steps, refusal, bond, timeout and unsaved take as `uncertain`, declined, identity
  missing, `--yes` required, removed spelling): each conforms to its command's checked-in schema, is reproduced by the owned constructors, carries data only when it should, and seventeen
  invalid variants do not conform.
- **Checks on the checks**: allowing whitespace in an invoice, mapping an unreadable store to missing and disabling the writable-file check each failed a test; restored.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked --no-fail-fast` (600 tests: lib 159, `cli_functions` 67, `integration_tests` 45, `order_info_process` 34, `order_list_process` 46,
  `order_new_process` 40, `order_take_process` 35, `parser_orders` 123, `parser_dms` 19, `parser_disputes` 9, `util_misc` 17, doctests 6), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all pass.

## Order Take, Slice 6: Docs And Verification: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `40f125c` (slice 5, not pushed when this was written; CI is green for the pushed slices up to 4b, Windows included).
Documentation only; no production change. Lean mode: the only evidence entry for the slice.

- **Docs**: `docs/architecture.md` (the legacy `take_order.rs` and its size row are gone, the shared write dispatch and the take modules are described, a "taking an order" flow
  is added), `docs/pow_error_handling.md` (the list of flows through `wait_for_dm` no longer names the migrated commands), the requirement evidence in `tests/README.md`,
  `specs/004-order-take-json/quickstart.md` (focused commands, offline smoke check, acceptance matrix, the six-step regtest probe procedure), and the status lines of the spec, plan,
  `AGENTS.md`, `docs/ai-development.md` and the roadmap (Amendment 12). The README, command reference and database notes were migrated in slice 4a; the boundary counts and CI
  wiring needed no change (the transport owners are still `msg`).
- **Offline smoke check** (the real binary, empty environment, sandboxed home, closed stdin): `order take-sell` in JSON without `--yes` is `invalid_arguments` exit 2 naming
  `order.take-sell`; `order take-buy --yes` with no configuration is `configuration_missing` exit 1; `takesell` is an unrecognized subcommand; nothing is created.
- Gates: fmt, Clippy `-D warnings`, `cargo test --locked` (600 tests), `cargo build --locked`, `cargo +1.94.0 check --locked --all-targets --all-features`: all pass (no source
  changed since slice 5).
- **Open**: the regtest probe (separate authorization, regtest identities only, never `devenv.sh clean`), the MCP migration (`create_order`, `take_order`, `get_order` all break against
  the CLI until then), Cashu escrow replies (reported as unusable), and the empty fields of the minimal row.

## Regtest Probe Of order info, order new And The Takes: 2026-11-01

Authorized by the maintainer on 2026-11-01. Regtest only: the MCP repository's `regtest/` network (bitcoind 27, three LND 0.18.3 nodes, nostr-rs-relay 0.9, **mostrod v0.19.3**), the
throwaway identities Alice and Bob, the debug build of `develop` at `aa20208` through `devenv.sh cli`. No public node, no `devenv.sh clean`, no identity file read (only
`last_trade_index` columns of the throwaway stores were read or, for the stale-index case, set).

- **Setup defect, not ours**: the first `devenv.sh up` failed with "mostro LND did not sync to chain": the chain left from 2026-10-20 had a stale tip, so bitcoind reported initial block
  download and LND waited. One `devenv.sh mine 1` fixed it and a second `up` opened the four channels.
- **`order list`**: `ok`, the book empty then one pending order after Alice's first `order new`, with `completed_relays` 1/1.
- **`order new`** (Alice, fixed sell order and a buy order; a range order 10..50; four orders in all): `ok`, `published/accepted/applied`; the reply is `new-order` with the order
  (`pending`, expiry about 24 h, net sats equal to the request), addressed to the trade key; the trade index advances 2, 3, 4, 6, 7 on Alice (5 was spent by a take, see below) and the node
  accepted each; a range order comes back with `fiat_amount` 0 and `min`/`max`.
- **`order info`**: the owner sees the order with `seller_trade_pubkey` filled in by the node; another identity asking for it gets `ok` with the id in `not_returned` and the
  `orders_not_returned` warning (the success the design maps both an empty reply and `not_found` to; the log did not show which the node sent).
- **`order take-sell` without an invoice** (Bob): `ok`, `add_invoice`, the order in `waiting-buyer-invoice`, `published/accepted/applied`, trade index 1 for a new identity. The legacy
  `addinvoice -o <id> -i <bolt11>` then accepted the row the new command saved, the hold invoice was paid by Alice, `fiatsent` and `release` ran, and `order info` showed `success`: the
  rows written by the new commands work with the old ones.
- **`order take-sell` with an invoice**: `ok`, `wait_for_seller_payment` with a null order, as the source says (#204); the minimal row was enough for `fiatsent` and `release` to complete the trade
  (`success`).
- **`order take-buy`** (Bob takes Alice's buy order): `ok`, `pay_hold_invoice`, the order in `waiting-payment`, a hold invoice of 616 characters in `data.payment_request`.
- **Range take**: `--amount` inside the bounds is accepted (`add_invoice`, the order shows the amount 30 of 10..50); none, 5 and 60 are refused `out_of_range_sats_amount`.
- **Refusals** (all `node_refused`, `published/rejected/not_attempted`, nothing spent): `pending_order_exists` (a taker who already has a pending order), `invalid_order_status` (an already taken
  order), `not_found` (unknown id), `invalid_trade_index` (local index set back to 1 while the node's was 4; the local index stayed 1; the legacy `getlasttradeindex` resynchronised it to 4).
  The checks run in the order of the source: the pending-order check comes before the side check.
- **Not as the fixture assumed**: taking one's own order **succeeds** when the take uses a fresh trade key (the node compares trade keys, not identities), so the `invalid_pubkey`
  refusal modeled by the scripted daemon is only reachable by reusing the maker's trade key; the CLI never does. Nothing in the CLI needed to change.
- **Node stopped during a take** (`docker stop` of mostrod): `uncertain` / `reply_timeout` after 15 s, `published/unknown/applied`, local index 5 spent. After the restart the order was still
  `pending` on the node (the missed event was not processed) and the next request, at local index 6 over the node's 4, passed the index check (the refusal that followed was
  `pending_order_exists`), so a gap in the indexes is harmless, as designed.
- **Not verified**: a bonded node (`bond_required`) and a node with Cashu escrow, because this regtest configures neither; an order outside its take window; a stale market price; the NIP-42
  AUTH path; behavior on mostrod v0.20.x (the scripted daemon is built from that source, the probe ran on v0.19.3 and they agreed on every case above).
- **State left behind**: the regtest network is still running with five orders and two completed trades; `regtest/devenv.sh down` stops it and keeps the state.

## Overview

**Total Test Files Created/Modified:** 6
**Total Test Functions:** 78 tests
**Testing Framework:** Rust's built-in test framework with tokio for async tests

## Test Coverage by File

### 1. `tests/parser_dms.rs` (16 tests)
Tests for the Direct Messages parser module, covering the significant changes to message display and handling.

#### Test Categories:

##### Basic Functionality (3 tests)
- `parse_dm_empty` - Verifies empty event parsing
- `print_dms_empty` - Verifies empty message list printing
- `print_dms_with_mostro_pubkey` - Tests Mostro pubkey identification

##### Message Types (8 tests)
- `print_dms_with_single_message` - Single message display
- `print_dms_with_text_payload` - Text message payload handling
- `print_dms_with_payment_request` - Payment invoice messages
- `print_dms_with_multiple_messages` - Multiple messages with various actions
- `print_dms_with_dispute_payload` - Dispute-related messages
- `print_dms_with_orders_payload` - Order information messages
- `print_dms_with_restore_session_payload` - Session restoration messages
- `print_dms_with_rating_action` - User rating messages

##### Edge Cases (5 tests)
- `print_dms_distinguishes_mostro` - Tests Mostro sender identification with emoji
- `parse_dm_with_time_filter` - Time-based filtering
- `print_dms_with_long_details_truncation` - Long text truncation (>120 chars)
- `print_dms_with_add_invoice_action` - Add invoice action display
- `print_dms_with_invalid_timestamp` - Invalid timestamp handling

**Key Changes Tested:**
- New table-based message display format
- Mostro sender identification (🧌 emoji)
- Action-specific icons and colors
- Details truncation for compact display
- New payload types (Orders, RestoreData)

---

### 2. `tests/cli_functions.rs` (26 tests)
Tests for CLI command functions and message creation logic.

#### Test Categories:

##### Rate User Functionality (3 tests)
- `test_get_user_rate_valid_ratings` - Valid rating values (1-5)
- `test_invalid_ratings_out_of_range` - Invalid ratings rejection
- `test_rate_user_message_creation` - Rating message structure

##### Orders Info Command (5 tests)
- `test_orders_info_empty_order_ids` - Empty order ID validation
- `test_orders_info_single_order_id` - Single order ID handling
- `test_orders_info_multiple_order_ids` - Multiple unique order IDs
- `test_orders_info_payload_creation` - Payload::Ids creation
- `test_message_creation_for_orders_action` - Orders action message

##### Restore Session (2 tests)
- `test_restore_message_creation` - Restore message structure
- `test_restore_message_serialization` - JSON serialization

##### Take Order Payloads (3 tests)
- `test_take_buy_payload_with_amount` - Amount payload for buy orders
- `test_take_sell_payload_with_invoice` - Invoice payload for sell orders
- `test_take_sell_payload_with_invoice_and_amount` - Combined payload

##### Dispute Actions (4 tests)
- `test_dispute_message_creation_add_solver` - Add solver message
- `test_dispute_message_cancel` - Cancel dispute
- `test_dispute_message_settle` - Settle dispute
- `test_dispute_message_take` - Take dispute

##### Send Message Actions (5 tests)
- `test_send_msg_cancel_action` - Cancel order action
- `test_send_msg_fiat_sent_action` - Fiat sent confirmation
- `test_send_msg_release_action` - Release sats action
- `test_send_msg_dispute_action` - Dispute initiation
- `test_dm_message_creation` - Direct message creation

##### Other Commands (4 tests)
- `test_new_order_message_with_trade_index` - New order with trade index
- `test_last_trade_index_message` - Last trade index request
- `test_rating_payload_creation` - Rating payload (1-5)
- `test_message_serialization_for_orders` - Message JSON serialization

**Key Changes Tested:**
- New `orders_info` command implementation
- Enhanced `restore` command with response handling
- Rating validation logic
- Improved message formatting
- All dispute admin actions

---

### 3. `tests/util_misc.rs` (13 tests)
Tests for utility functions, particularly the critical path change in `get_mcli_path`.

#### Test Categories:

##### uppercase_first Function (9 tests)
- `test_uppercase_first_empty_string` - Empty string handling
- `test_uppercase_first_single_char` - Single character
- `test_uppercase_first_already_uppercase` - Already capitalized
- `test_uppercase_first_lowercase_word` - Lowercase conversion
- `test_uppercase_first_multiple_words` - Multi-word strings
- `test_uppercase_first_special_chars` - Special character handling
- `test_uppercase_first_unicode` - Unicode character support (über → Über)
- `test_uppercase_first_numeric` - Numeric prefix
- `test_uppercase_first_whitespace` - Leading whitespace

##### get_mcli_path Function (3 tests)
- `test_get_mcli_path_returns_valid_path` - Valid path with `.mcli`
- `test_get_mcli_path_is_absolute` - Absolute path verification
- `test_get_mcli_path_consistent` - Consistency across calls

---

### 4. `tests/parser_orders.rs` (11 tests - 8 new)
Enhanced tests for order parsing and display.

#### Existing Tests (3 tests)
- `parse_orders_empty` - Empty event handling
- `parse_orders_basic_and_print` - Basic order parsing
- (with currency, status, and kind filters)

#### New Tests (8 tests)

##### Filter Validation (3 tests)
- `parse_orders_with_kind_filter` - Buy/Sell kind filtering
- `parse_orders_with_status_filter` - Status-based filtering
- `parse_orders_with_currency_filter` - Currency filtering

##### Multi-Order Handling (3 tests)
- `parse_orders_no_filters` - All orders without filters
- `print_orders_empty_list` - Empty order list display
- `print_orders_multiple_orders` - Multiple order display

##### Edge Cases (2 tests)
- `parse_orders_different_amounts` - Various amount values (10k-1M sats)
- `parse_orders_different_currencies` - Multiple currencies (USD, EUR, GBP, JPY, CAD)
- `parse_orders_market_price` - Market price orders (amount = 0)

**Key Changes Tested:**
- Enhanced table formatting with icons (📈, 💰, 💱, etc.)
- Colored status indicators (Active/Green, Pending/Yellow, etc.)
- "No offers found" message improvements
- Market price order handling

---

### 5. `tests/parser_disputes.rs` (9 tests - 6 new)
Enhanced tests for dispute parsing and display.

#### Existing Tests (3 tests)
- `parse_disputes_empty` - Empty dispute list
- `parse_disputes_basic_and_print` - Basic dispute parsing

#### New Tests (6 tests)

##### Status Handling (4 tests)
- `parse_disputes_multiple_statuses` - All status types (Initiated, InProgress, Settled, Canceled)
- `parse_disputes_initiated_status` - Initiated status
- `parse_disputes_settled_status` - Settled status
- `parse_disputes_canceled_status` - Canceled status

##### Display & Validation (2 tests)
- `print_disputes_empty_list` - Empty dispute list message
- `print_disputes_multiple_disputes` - Multiple dispute display
- `parse_disputes_unique_ids` - UUID uniqueness verification

**Key Changes Tested:**
- Enhanced table with icons (🆔, 📊, 📅)
- Status color coding (Yellow/pending, Green/settled, Red/canceled)
- "No disputes found" message improvements
- Multiple status types in one test

---

### 6. `tests/integration_tests.rs` (3 tests - existing)
Integration tests for context creation and setup.

**Tests:**
- `test_context_creation` - Context initialization
- `test_context_fields_are_valid` - Field validation
- `test_filter_creation_integration` - Filter creation for event fetching

**Note:** These tests were not modified but remain valid for integration testing.

---

## Test Execution

### Run All Tests
```bash
cargo test
```

### Run Specific Test File
```bash
cargo test --test parser_dms
cargo test --test cli_functions
cargo test --test util_misc
cargo test --test parser_orders
cargo test --test parser_disputes
```

### Run Tests with Output
```bash
cargo test -- --nocapture
```

### Run Specific Test
```bash
cargo test test_orders_info_empty_order_ids
```

---

## Code Coverage Summary

### Changed Files Tested

| File | Lines Changed | Test Coverage |
|------|---------------|---------------|
| `src/parser/dms.rs` | ~500 lines | ✅ Comprehensive (16 tests) |
| `src/cli/orders_info.rs` | 77 lines (NEW) | ✅ Full coverage (5 tests) |
| `src/cli/rate_user.rs` | +7 lines | ✅ Covered (3 tests) |
| `src/cli/restore.rs` | +65 lines | ✅ Covered (2 tests) |
| `src/cli/take_dispute.rs` | +135 lines | ✅ Covered (4 tests) |
| `src/cli/new_order.rs` | +70 lines | ✅ Covered (1 test) |
| `src/cli/take_order.rs` | +55 lines | ✅ Covered (3 tests) |
| `src/parser/orders.rs` | +69 lines | ✅ Enhanced (8 new tests) |
| `src/parser/disputes.rs` | +26 lines | ✅ Enhanced (6 new tests) |
| `src/util/misc.rs` | 1 line | ✅ Critical path tested (13 tests) |
| Other CLI files | ~200 lines | ✅ Message creation tested |

### Test Types Distribution

- **Unit Tests:** 75 tests (97%)
- **Integration Tests:** 3 tests (3%)
- **Async Tests:** 16 tests (21%)
- **Sync Tests:** 62 tests (79%)

---

## Key Testing Patterns Used

### 1. **Message Creation Pattern**
```rust
let message = Message::new_order(
    Some(order_id),
    Some(request_id),
    Some(trade_index),
    Action::Orders,
    Some(payload),
);

let inner = message.get_inner_message_kind();
assert_eq!(inner.action, Action::Orders);
```

### 2. **Payload Validation Pattern**
```rust
match payload {
    Payload::Ids(ids) => {
        assert_eq!(ids.len(), expected_len);
        // Further validation
    }
    _ => panic!("Expected Payload::Ids"),
}
```

### 3. **Event Building Pattern**
```rust
fn build_order_event(kind, status, fiat, amount, fiat_amount) -> Event {
    let keys = Keys::generate();
    // Build event with tags
}
```

### 4. **Async Testing Pattern**
```rust
#[tokio::test]
async fn test_async_function() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

---

## Edge Cases Covered

### 1. **Empty Collections**
- Empty order lists
- Empty dispute lists
- Empty message arrays
- Empty order ID vectors

### 2. **Invalid Input**
- Out-of-range ratings (0, 6, 255)
- Invalid timestamps
- Missing required fields
- Null/None values

### 3. **Boundary Conditions**
- Single item collections
- Maximum length strings (>120 chars truncation)
- Market price orders (amount = 0)
- Multiple simultaneous actions

### 4. **Data Integrity**
- UUID uniqueness
- Message serialization/deserialization
- Path consistency
- Type conversions (u32 → i64)

---

## Dependencies Verified

All tests use only existing dependencies:
- `tokio` (async runtime)
- `tokio-test` (testing utilities)
- `rstest` (parametric testing)
- `serial_test` (serialization)
- `mostro-core` (core types)
- `nostr-sdk` (Nostr protocol)
- `uuid` (UUID generation)
- `anyhow` (error handling)

**No new dependencies introduced.**

---

## Best Practices Followed

1. ✅ **Descriptive Test Names** - Clear purpose in test name
2. ✅ **AAA Pattern** - Arrange, Act, Assert
3. ✅ **Single Responsibility** - One concept per test
4. ✅ **Independent Tests** - No test depends on another
5. ✅ **Fast Execution** - No network calls or heavy I/O
6. ✅ **Deterministic** - Same input always produces same output
7. ✅ **Comprehensive Coverage** - Happy paths, edge cases, failures
8. ✅ **Documentation** - Clear comments for complex logic

---

**Testing Strategy:**
- 4 dedicated tests verify the new path
- Path consistency across multiple calls verified
- Home directory integration confirmed

---

## Recommendations

### 1. **Consider Adding:**
- Integration tests for async CLI commands (requires mock server)
- Property-based tests using `proptest` for fuzz testing
- Performance benchmarks for parser functions
- Database migration tests for path change

### 2. **Future Enhancements:**
- Add tests for error message formatting
- Test color output rendering (currently visual only)
- Add tests for table width calculations
- Test Unicode emoji rendering

---

## Conclusion

This test suite provides **comprehensive coverage** of all changes in the branch:

- ✅ **78 tests** covering all major functionality
- ✅ **100% of new files** have test coverage
- ✅ **All modified functions** have corresponding tests
- ✅ **Edge cases and error conditions** thoroughly tested
- ✅ **No new dependencies** introduced
- ✅ **Best practices** consistently applied

The tests are:
- **Maintainable** - Clear, simple, well-documented
- **Reliable** - Deterministic and fast
- **Comprehensive** - Happy paths, edge cases, failures
- **Actionable** - Clear failure messages

All tests follow the project's established patterns and integrate seamlessly with the existing test infrastructure.
## Correction: Bond Invoices Are Returned, Not Dropped: 2026-11-01

A user took an order on a bonded local node with `order take-sell --yes --output json` and got `bond_required` with no way to pay. The node's
`pay-bond-invoice` carries the parked order and the bond invoice; slices 3 and 4 of `order new` and `order take` discarded both and reported an error.

- **Fix**: `order take-*` returns `next_step` `pay_bond_invoice` with the bond invoice in `payment_request`; `order new` gains `next_step`
  (`wait_for_taker` | `pay_bond_invoice`) and `payment_request`. Both are `ok`, and save the parked order with the trade index in one transaction. The error code
  `bond_required` and the accepted-but-not-live stage combinations are removed from the schemas, discovery and rules. Human text shows the bond invoice alone on
  its line and a maker or taker wording. Specs 003 and 004 carry an amendment; their examples, data models and quickstarts are updated.
- **Tests (rewritten first, then green)**: bonded process and binary tests for `order new` and `order take-sell` now expect `ok`, the bond invoice, the order and
  `(1 row, index 1)`; unit tests for `from_bond` and `Taken::PayBond`; schema fixtures regenerated; examples `bond_to_pay` and `sell_pay_bond_invoice` replace
  `bond_required` and `sell_bond_required` and are reproduced by the constructors.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 602 passed and 0 failed (checked on the FAILED lines), build, MSRV 1.94.0.
- **Open**: the real bonded local node was not probed yet (needs authorization); how a user continues after paying the bond (reading the node's next message) is
  not migrated: `getdm` is legacy and human-only.

## Add Invoice, Slice 1: Characterization And Fixtures: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `e828903` (pushed). One behavior-preserving extraction in production code (below); lean mode: the only
evidence entry for the slice.

- **Legacy `addinvoice`/`addbondinvoice` pinned**, removed with the commands in slice 4:
  - **Payload** (4 unit tests, after extracting `invoice_payload` / `bond_payload` from the two `execute_*` functions without changing behavior):
    `addinvoice` sends a Lightning address as given in a bare `PaymentRequest(None, invoice, None)`; anything that is neither an address nor a parseable,
    unexpired BOLT-11 is refused locally with `Invalid invoice: ...`; `addbondinvoice` refuses a Lightning address and garbage. The accepted BOLT-11 path
    (canonical re-serialization) is not unit-tested: no expiry-proof vector exists without a signing dependency; slice 3 covers it through the scripted daemon.
  - **Grammar** (4 tests, `cli_functions::invoice_characterization`): `-o/--orderid` (the long flag is squashed, `--order-id` is unknown) and `-i/--invoice`,
    both required, the order id a UUID in any case, the invoice unchecked by clap; both registered legacy with `order.add-invoice` / `bond.add-invoice`, no JSON.
  - **Read from source, not tested**: `addbondinvoice` treats `WaitForDmTimeout` as success (the `Err(e) if e.downcast_ref::<WaitForDmTimeout>()` arm); only
    `waiting-seller-to-pay` updated the local row (already pinned by the existing `print_commands_results` tests).
- **Fixture** (`tests/common/fake_mostro.rs`, `tests/invoice_process.rs`): `invoice_daemon(ledger, acks)` applies the source's rules: the order found by id, only
  the buyer's trade key may answer (`invalid_peer`), the invoice validated (`invalid_invoice`), then by status: `waiting-seller-to-pay` (no payload, request id
  echoed, status to `waiting-payment`), `hold-invoice-payment-accepted` with the order (preimage stored), `invoice-updated` (settled order), else
  `not_allowed_by_status`; for bonds: only the open bond's recipient, the invoice must be exactly the share, then `bond-invoice-accepted` with an order and
  **no request id** (`acks`), or silence (older node). Neither request carries a trade index.
- **Self-tests** (9): each reply and its addressing and state change, every refusal leaving the ledger untouched (unknown order, another key, no invoice, bad
  invoice, wrong status; wrong amount, address, no bond, another key), a second bond invoice refused, the ack with no request id, silence on an older node, any
  other action refused. Two mutation spot checks (ack echoing the request id; the buyer check removed) are caught by their tests.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 619 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Add Invoice, Slice 2: Store, Correlation And Outcomes: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `ea47000` (pushed). Lean mode: the only evidence entry for the slice. Registry and generated schemas are not
touched until slice 4 (the legacy entries stay legacy, as in the earlier specs).

- **Correlation by order id** (`util/reply.rs`, `msg/exchange.rs`): `ExpectedReply::or_by_order(order, actions)` / `Expect::by_order` also accept a message with **no
  request id** that names the order and one of the given actions, because the node sends `bond-invoice-accepted` that way. A different request id, another
  order, a missing order and an action outside the list are never accepted this way, and without the option an id-less message is never accepted. 3 unit
  tests; the existing request-id tests are unchanged.
- **Store** (`store.rs`): `order_keys(id)` reads the stored trade key (a missing order, a null key or an unusable key is `None`; a missing database is `Missing`;
  it writes nothing and never creates the file) and `set_status(id, status)` updates one order and fails for an unknown one; neither touches the trade index.
  6 tests in `tests/invoice_process.rs::store`, including byte-identical files after reads and after a failed update.
- **Codes**: error `order_unknown` (exit 1) and warning `local_status_stale`, each scoped to the invoice commands; the contract tables of `outcome.rs` list them.
- **Stage vocabularies** (`outcome/rules.rs`): `Stages::Update` (order: success `applied`, or `failed` with the warning; everything else `not_attempted`; uncertain
  with `not_attempted`) and `Stages::Delivery` (bond: persistence is always `not_attempted`).
- **Outcomes** (`outcome/invoice.rs`, 15 tests): `InvoiceInput::validate` (order: BOLT-11 or address; bond: BOLT-11 only; trimmed, bounded, no control or
  whitespace; the error never echoes the value and `Debug` never shows the invoice), the payload (a bare payment request as given), `OrderInvoiceData::from_answer`
  (result, the status to record, a reply about another order or without an id is unusable) and `BondInvoiceData::from_ack` (the share when the node gave one).
  Generated schemas for both names conform for real outcomes (ok, stale, uncertain per code, refusal, `order_unknown`), the bond schema has no store code and
  allows no local write, and contradictory outcomes do not conform.
- **Mutation spot checks** (3): the order-id match without the order check, an address accepted for a bond, and a status update that ignores a missing order are each
  caught.
- **Gates**: fmt, clippy `-D warnings` (one `large_enum_variant` fixed by boxing the order), `cargo test --locked --no-fail-fast` 644 passed and 0 failed (no FAILED
  line), build, MSRV 1.94.0.

## Add Invoice, Slice 3: The Operations: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `fb66dbd` (pushed). Lean mode: the only evidence entry for the slice. No CLI wiring yet (slice 4).

- **Code**: `commands/order/invoice.rs` has `order_add_invoice` and `bond_add_invoice` over one private `send`: look up the order's stored trade key (an unknown
  order or a missing/unusable key is `order_unknown`, nothing sent), publish one signed reply with the order id and **no trade index**, wait through
  `msg::exchange` (for the bond the allowed set adds the order-id match for `bond-invoice-accepted`), map the answer. After an accepted `order.add-invoice` the
  local status follows the reply (`waiting-seller-to-pay` to `waiting-payment`, an accepted payment to the order's status, an update to nothing); a failed
  update is the `local_status_stale` warning with persistence `failed`, not a failure. `Stages::Update` gained the success combination "published, accepted,
  not_attempted" for a reply that calls for no update.
- **Tests** (15, `tests/invoice_process.rs::operation`, against the scripted node and a real seeded store):
  - `order.add-invoice`: the three accepted replies and their local effect (status, order returned, nothing changed for an update, the trade index unchanged,
    one request without an index and with a valid signature); every refusal (`not_allowed_by_status`, `invalid_peer`, `not_found`) as `node_refused` with the
    node's reason and a byte-identical database; silence as `uncertain` with no retry; five unusable replies as `reply_malformed`; decoys (stale request id,
    another author, another addressee, no request id) ignored; a forced status-update failure (an `UPDATE` trigger) as `ok` with `local_status_stale`; an unknown
    or keyless order sending nothing.
  - `bond.add-invoice`: the ack with no request id accepted through the order-id match, with the share and no byte written; an older node's silence as `uncertain`
    although the node did accept; refusals (`invalid_invoice`, `not_allowed_by_status`); decoy acks (wrong order, no order, stale request id) ignored, an
    ack for another order alone ending in a timeout, an unusable ack as `reply_malformed`; an unknown order sending nothing.
  - Work the node expects (`pow_required`) stops both commands before anything is sent or written.
- **Mutation spot checks** (3): the bond without its order-id match, the stale-status warning removed, and `waiting-seller-to-pay` recording no status are each caught.
- **Gates**: fmt, clippy `-D warnings` (a `type_complexity` in the new tests fixed with an alias), `cargo test --locked --no-fail-fast` 659 passed and 0 failed
  (no FAILED line), build, MSRV 1.94.0.

## Add Invoice, Slice 4a: The Commands: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `0070511` (pushed). Lean mode: the only evidence entry for 4a; the binary scenarios are 4b.

- **Grammar**: `order add-invoice --order UUID --invoice INVOICE [--yes]` and a new `bond add-invoice --order UUID --invoice BOLT11 [--yes]` group
  (`BondCommands`, `AddInvoiceArgs`, `AddBondInvoiceArgs`). `addinvoice`, `addbondinvoice`, `-o`, `--orderid`, `--order-id` and `-i` are gone with no alias;
  `cli/add_invoice.rs` and `cli/add_bond_invoice.rs` (and their `execute_*` and unit tests) are removed. 4 grammar tests replace the 4 characterization tests
  (options required, UUID in any case, old spellings rejected, legacy names unregistered, both registered as write commands with the approved names).
- **Dispatch**: `cli/invoice.rs` (`prepare` shared by both: the `--yes` rule, validation, `resolve_base`, the order lookup for the preview and the question) over
  `cli/write.rs`, whose `resolve` now builds on `resolve_base(cli, writable)`; the store is writable for the order invoice only. The preview shows the order and the
  trade public key, never the invoice. `parse_human/invoice.rs` renders the preview and both results.
- **Registry and discovery**: `order.add-invoice` and `bond.add-invoice` are `Write` entries at `["order","add-invoice"]` and `["bond","add-invoice"]` with generated
  schemas (`OutcomeRules::invoice`) and the machine configuration names; fixtures `docs/schemas/order-add-invoice-v1.json`, `bond-add-invoice-v1.json` and the
  regenerated discovery, with a drift test. The legacy samples in the output-intent and failure-matrix tests moved to `fiatsent`.
- **Found and fixed on the way**: the generated `ErrorCode` enums of `order new` and both takes listed `pow_required` and `request_rejected` twice (an error list
  merged with the uncertain list); `scoped_codes` now keeps each code once and those fixtures are regenerated.
- **Hints**: the human hints that named `addinvoice` and `addbondinvoice` (take-sell's next step, the bond payout request) now name the new commands.
- **Docs**: README, command reference and architecture describe the new commands and drop the old ones.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 656 passed and 0 failed (no FAILED line), build, MSRV 1.94.0. Mutation checks and the
  binary scenarios follow in 4b.

## Add Invoice, Slice 4b: Binary Scenarios: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `75bcc6a`. Lean mode: the only evidence entry for 4b. The real binary runs against the scripted node and a seeded
sandbox store (`tests/invoice_process.rs::operation`, 9 scenarios).

- **Accepted, JSON**: `order add-invoice` exits 0 with one document that conforms to the checked-in schema, `waiting_seller_payment`, stages published/accepted/applied,
  the local status `waiting-payment`, the trade index unchanged, one request carrying the invoice, `--verbose` printing only `verbose: order.add-invoice ok`; the
  trade key, the invoice and the relay address appear in no stream. `bond add-invoice` exits 0 with `invoice_accepted` and the share, persistence `not_attempted`,
  and the sandbox directory byte-identical.
- **Confirmation**: JSON without `--yes` is `invalid_arguments` (exit 2) for both commands, with nothing sent and nothing written; the human prompt accepts `y`,
  blank and `Y\r\n` and declines `n`, `no`, anything else, empty input and a closed stdin (exit 1, `order_declined`, nothing sent); the preview names the order and
  the trade key and never the invoice; the bond preview and result show the share.
- **Unknown order**: fails before any question in both modes (`order_unknown`, fixed line on stderr, nothing sent, nothing written).
- **Refusal**: `node_refused` with the reason in JSON and in the human line, exit 1, one request per run, nothing written.
- **Silence**: a silent (older) node is `uncertain` / `reply_timeout` after the production reply budget, never retried; the human run prints the bond-specific
  advice. (This also showed the older node *had* accepted: a second invoice to the same node is then refused, which is why a fresh node is used for the human run.)
- **Stale local status**: a forced update failure is still exit 0 with persistence `failed` and the warning `local_status_stale`.
- **Invalid input**: a malformed order invoice and an address for a bond are `invalid_arguments` before anything happens and are never echoed.
- **Mutation spot checks** (2): the `--yes` rule removed and the bond share line removed are each caught.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 665 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Add Invoice, Slice 5: Failure Matrix And Examples: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `05eb60b` (pushed). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (`tests/invoice_process.rs::operation`, 8 tests, the real binary), each case run as JSON, human and `--verbose` on **both commands**, checking the
  code and exit, that nothing was sent, that the sandbox is byte-identical and that no file appears elsewhere:
  - **Before any request**: no store, no user, a file that is not a database, no `users` table, a null mnemonic, no node key, no relays, a bad node key, a bad relay,
    a bad `--pow`, a bad `--transport`: each with its stable code, the pre-publication stages and no sentinel, relay address or invoice in any stream.
  - **Store**: only `order add-invoice` needs a writable store (`store_unwritable` on a read-only file, before anything is sent); `bond add-invoice` still succeeds
    on it because it writes nothing. (Skipped by the test when the process ignores file modes.)
  - **Node that cannot be served**: protocol 1 and more required work stop both commands before publishing; an explicit `--transport` and enough `--pow` pass.
  - **Relays**: a relay rejecting the write is `request_rejected` with nothing changed; a dead relay is `relay_connection_failed`, with no address shown.
  - **Invalid input** (11 invocations): an oversized invoice, whitespace, `lnurl1...`, a bad UUID, a missing option, an address for a bond, text that is no invoice, the
    removed `addinvoice`/`addbondinvoice` spellings and the hyphenated `--order-id`: exit 2, `invalid_arguments`, the command named when it resolved, nothing
    echoed, nothing sent, store unchanged.
  - **Streams**: an unwritable stdout after the invoice exits 1 with one fixed line per mode and for both commands (the invoice was sent and accepted all the
    same); an unwritable stderr changes neither the exit nor the document.
- **Examples** (`specs/005-invoice-json/contracts/examples.json`, 15 cases generated from the owned constructors, not written by hand): every accepted result,
  the stale local status, a refusal, both uncertain endings, `order_unknown` for each command, the declined and `--yes` cases and the removed spelling. 4 tests in
  `integration_tests.rs`: all conform to the checked-in schemas with the right exit code and data presence, the constructors reproduce them byte for byte, and
  17 edited variants (unknown result, uppercase key, bad id, missing key, wrong status or stages, a store error on a bond, a numeric share, ...) do not conform.
- **Mutation spot checks** (3): the order invoice not requiring a writable store, a bond requiring one, and a changed unwritable-stdout line are each caught.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 676 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Add Invoice, Slice 6: Documentation, Boundaries, Verification: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `6c2020a` (pushed; CI was green through slice 4b with the Windows runner, slice 5 was running). Documentation only;
no production change.

- **Docs**: architecture (modules, `resolve_base`, the order-id match, the two stage vocabularies), command reference (both commands, the MCP `add_invoice` break and
  the retired "timeout means success" reading), README (usage, the JSON command list), `AGENTS.md` and the resume prompt in `docs/ai-development.md`.
- **Validation guide** (`specs/005-invoice-json/quickstart.md`): focused commands, an offline smoke check (run and confirmed: exit 2, 1, 2, 2 and an empty home), the
  acceptance matrix mapped to tests, and the regtest probe procedure for `order add-invoice`.
- **Requirement evidence** (`tests/README.md`, "Add Invoice: Requirement Evidence"): FR-001 to FR-010 and SC-001 to SC-005 verified on Linux against the scripted node;
  SC-006 stays **open**.
- **Open items**: the regtest probe of `order add-invoice` (needs authorization; `bond add-invoice` cannot be probed on regtest, which has no bonds, so the
  acknowledgement with no request id remains an assumption from the v0.20.1 source); the MCP `add_invoice` migration; the status update on
  `hold-invoice-payment-accepted`, which the legacy command never did; and, for the whole flow, a command that reads the node's messages.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 676 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Regtest Probe Of order add-invoice (And The Bond Invoice Of order take): 2026-11-01

Authorized by the maintainer on 2026-11-01, including the use of **public relays** (`wss://mostro-p2p.tech`, `wss://relay.shadowbip.com`) because the maintainer's local regtest
node, with a **taker bond**, is reachable only through them. Two fresh throwaway identities (a maker and a taker) in temporary directories of the session, the debug build of `develop` at
`dbcf72d`; the maintainer's own store was never used. One test order (payment method `TEST-mostro-cli-probe`, 5 USD, market price) was created, then cancelled at the end; the throwaway
stores were deleted. The node's version was not queried. The maintainer paid the bond and created the buyer invoice in Polar; no macaroon, certificate or key was shared or read.

- **Why not the MCP regtest network**: `devenv.sh up` failed twice (the image build could not resolve Docker Hub from the build container) and, started from the cached image, its
  containers were unreachable from the host (every Docker bridge address timed out, the published relay port included; `ufw` is active and the cause needs root). Left alone and stopped again.
- **`order new`** (maker): `ok`, `wait_for_taker`, `published/accepted/applied`, the order `pending` with 0 sats (market price), trade index 1.
- **`order take-sell`** (taker) on the bonded node: `ok`, **`pay_bond_invoice`**, the parked order `pending`, a 351-character bond invoice in `data.payment_request`, the row and index
  saved. This is the first check of the bond fix against a real node: the old build would have reported an error and dropped the invoice. The amount in this reply was 1000 sats; the
  `add-invoice` that followed the payment carried 6233 sats (the price applied after the bond), so the bond reply's amount is not the amount to invoice.
- **After the bond was paid** (read with the legacy `getdm`, the only way until `message list` exists): `add-invoice` with the order (6233 sats, `waiting-buyer-invoice`), 2 minutes after the take.
- **`order add-invoice`** with a 6233-sat invoice: `ok`, `waiting_seller_payment`, `published/accepted/applied`, the local status `waiting-payment`, the trade index unchanged (1). The reply was
  matched by request id, so this node echoes it.
- **Refusals**, all `node_refused`, `published/rejected/not_attempted`, local status unchanged: the same invoice again (`not_allowed_by_status`), the maker answering for the taker's order
  (`invalid_peer`), an invoice the node cannot parse (`invalid_invoice`, answered at a status that would otherwise be `not_allowed_by_status`, which agrees with the source: the invoice is
  validated before the status).
- **Cleanup**: the maker's legacy `cancel` cancelled the order; `order info` showed `canceled` and the book held only the maintainer's own order again.
- **Not verified**: `payment_accepted` (needs the seller to have paid first) and `invoice_updated` (needs a settled order); a valid invoice of the wrong amount; the node stopped during the request
  (not done on someone else's node); and **all of `bond add-invoice`**, which needs a slashed bond. The acknowledgement without a request id therefore remains an assumption from the v0.20.1 source.
- **Nothing in the CLI needed to change.**

## Message List, Slice 1: Characterization And Fixtures: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `22588e0` (pushed). No production change. Lean mode: the only evidence entry for the slice.

- **Legacy `getdm` pinned**, removed with the command in slice 4:
  - **Parser** (7 tests, `message_list_process::legacy_getdm`, over the public `parse_dm_events`): messages come back oldest first whatever the order of the events; `--since` is minutes
    before now (200, 20 and 5 minutes old against windows of 300, 30 and 10), and a negative window is a cutoff in the future that drops everything; an event the key cannot open is
    skipped silently; the time and the sender of each message are reported; a body that is not Mostro JSON and any kind other than 14 are skipped. **The author is not checked by
    the parser**: an event signed by another key but addressed to the trade key is opened, so today only the relay filter (`author = node`) keeps impostors out. The new command must
    check the author itself.
  - **Grammar** (4 tests, `cli_functions::getdm_characterization`): `-s/--since` (default 30, any integer, zero and negative included) and `-f` with the long flag **squashed to
    `--fromuser`** (`--from-user` is unknown); registered legacy as `message.list` with no JSON.
  - **Read from source, not tested**: the key loop (one fetch per index from 1 to the context's trade index, a seven-day window and a 15 s timeout each, so N keys are N sequential
    reads), `--from-user` decrypting peer chat, and the saves of the counterparty and solver pubkeys (already covered by the existing `persist_*` tests in `parser/dms.rs`).
- **Mailbox fixture** (`tests/common/fake_mostro.rs`): `node_event(node, to, message, at)` builds the event a real node would leave on a relay (authored by the node, encrypted for the
  trade key, tagged with it, **created at a chosen time**, re-signed), and `mail_*` constructors for the messages of the flow: `pay-bond-invoice`, `add-invoice`, `pay-invoice`,
  `waiting-seller-to-pay`, `cant-do`, the bond payout request and a free-text message. The scripted node answers requests; this is for what a node volunteers.
- **Self-tests** (5, `message_list_process::mailbox`): a node event opens with the addressed key and with no other, is authored by the node, verifies and keeps its time; every
  constructor round-trips with the right action and order; the bond payload arrives intact; **one filter with several `p` keys returns exactly those keys' messages from the node and
  nothing from an impostor or another key**, and its `since` drops older messages (the filter the new command will use, against the relay the tests use).
- **Mutation spot checks** (2): the fixture ignoring the given time, and dropping the recipient tag, are each caught by two tests.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 692 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Message List, Slice 2: Keys, Records And Outcomes: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `8f45144` (pushed). Lean mode: the only evidence entry for the slice. No command wiring yet (slice 4).

- **Store** (`store.rs`): `Store::trade_keys(newest)` derives the keys of the newest indexes up to the recorded last index (newest first) and counts the older ones; a trade whose order
  was never saved still has a key. Read-only, never creates, and its `Debug` shows indexes only. New `StoreError::Unreadable` (a store that opens but cannot be used) with the code
  `store_unreadable`. 6 tests in `message_list_process::store`: all indexes newest first and distinct, the derivation equals the one `next_trade` uses, the caps (3 of 10 leaves 7, more than
  there are, zero), no trade yet, a byte-identical file and no secret in `Debug`, and missing store, missing user, a file that is not a database and an unusable mnemonic.
- **Codes and schema helpers**: error `store_unreadable` (exit 1) and warning `trade_keys_limited`, scoped to `message.list`; `schema::nullable_uuid`.
- **Records** (`outcome/messages.rs` and `outcome/messages/payload.rs`, 12 tests): `MessageListInput::validate` (1 to 10080 minutes, default 30, zero, negative and over-range refused without echoing),
  `NextStep` (only the four actions that ask something), `MessageRecord::from_message` (event id, RFC 3339 and Unix time, order id, request id, wire action, trade index and key, payload) and the
  payload summary: order, payment request (the bond or hold invoice is data), bond payout request with the slash time, refusal reason, counterparty pubkey, bounded text, amount, and any other
  variant by name only; an order without an id anywhere is not shown as an order; a time outside four-digit years is the aggregate warning and a null field.
- **Schema**: generated with `OutcomeRules::read` (snapshot on success, read stages only, no uncertain status, no store-write code); real outcomes with every payload kind conform; 12 contradictory
  edits (bad event id, uppercase key, numeric index or count, unknown next step or payload kind, uppercase action, missing flag, a write stage, no snapshot, another command) do not.
- **Found by the gates**: the boundary test refused `messages.rs` at 431 production lines; the payload summary moved to `outcome/messages/payload.rs`. (The first run after the fix compiled a test
  module that was missing imports and printed 0 failures with no passes: the compile errors are now counted as well.)
- **Mutation spot checks** (3): the key window off by one, a zero-minute window allowed, and unbounded text each caught.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 709 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Message List, Slice 3: The Read And The Operation: 2026-11-01

Native Linux, rustc 1.97.0, on `develop` after `e64119e` (pushed). Lean mode: the only evidence entry for the slice. No CLI wiring yet (slice 4).

- **Code**: `msg/inbox.rs` (`read_inbox`: one filter of kind 14, the node as author, every key as recipient, the window and the 50-per-relay limit, through `fetch_msg`; `open_events`: each event
  is opened with the key it names after the author, kind, time and recipient are checked **here**, not left to the relay; oldest first, the newest 200 kept; nothing asked when there are no keys,
  because a filter without a recipient would read everyone's mail) and `commands/message/list.rs` (`message_list`: the keys from the store, or the one stored for `--order` with its index when it
  is among the newest 200 and `null` otherwise; the records, `truncated`, `keys_read`/`keys_not_read`, the `trade_keys_limited` warning; failures with the evidence they have).
- **Tests** (15 operation tests against the mailbox, 2 unit tests of the opening):
  - the whole flow in order: seven messages of every kind oldest first with the right `next_step` and trade index, the bond and hold invoices and the payout request as data, read stages,
    snapshot 1/1 and 7 events, no warning, **the store byte-identical**, and the same answer when read twice;
  - what must not appear: a message signed by someone else, for another key, older than the window, or not Mostro JSON; the window deciding 1, 2, 2 and 3 messages for 10, 30, default and 300 minutes;
    the same event from two relays once, with 2/2 completed;
  - `--order`: only that key's messages with their index; an unknown order asks nothing (no relay connection); an order whose key is beyond the cap is read with an unknown index;
  - the limits: 250 trades read the newest 200 keys (the message for key 50 is out of reach), reported as `200`/`50` with `trade_keys_limited`; 60 messages in one relay come back as the newest 50
    with `truncated`; five relays of 50 give 250 and the newest 200 are kept; no trade yet is an empty success with no relay contact and 0 relays completed; an empty window is a success;
  - failures: a silent relay is `read_timeout` with a snapshot, a dead one `relay_connection_failed` with none, a missing store `identity_missing`, a damaged one `store_unreadable`, nothing written.
- **Mutation spot checks** (6): the local time bound, the empty-key guard and the record cap are each caught; the explicit author check and the sender check each survive alone because they
  back each other up (opening with a key already shows the sender) and **fail together**, which is the intended redundancy.
- **Found by the gates**: one test compared two reads including the snapshot's `since`, which moves with the clock; it now ignores `since` and `observed_at` (a test defect, not a product one).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 726 passed and 0 failed (no FAILED line), build, MSRV 1.94.0.

## Message List, Slice 4a: The Command: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `e9ea42b` (pushed). Lean mode: the only evidence entry for the slice. Binary scenarios are slice 4b.

- **Code**: `Commands::Message` / `MessageCommands::List(--since, --order)` replace `getdm` (removed with no alias, `-s` and `--from-user` with it); registry `["message","list"]` as `Read`
  with the existing canonical name; `cli/message.rs` (validate, `resolve_base` read-only, `message_list`, human or JSON, `--verbose` one code line); `parse_human/message.rs`
  (header with window and keys read, one block per message, each invoice alone on its line, the next step in words with the command that continues it, node text escaped, limits stated);
  generated schema (`OutcomeRules::read`, the `trade_keys_limited` warning) and the new fixtures `docs/schemas/message-list-v1.json` and regenerated `discovery-v1.json`.
  `getadmindm` keeps `execute_get_dm`. The hints of `order new` / `order take-*` now say `message list`.
- **Tests**: 5 grammar tests replace the 5 `getdm` characterization tests (defaults unset, long options only, range left to the operation, the old names and `--from-user` rejected, registered as a
  read); 6 renderer tests; the registry, legacy-JSON and discovery tests count `message.list` as machine-capable; the schema fixture and the discovery fixture match the generated documents.
- **Behavior changed on purpose**: `getdm` stored the counterparty and solver keys it saw; `message list` is read-only and returns them as data. `disputechat` reads the solver from the store, so
  the solver's key (the `peer` payload of `admin-took-dispute`) is now passed to `getdmuser` / `dmtouser`; the README and the `disputechat` hint say so.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 734 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0 (see the commit).

## Message List, Slice 4b: The Binary Scenarios: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `eaff4da` (pushed). Lean mode: the only evidence entry for the slice.

- **Tests** (7 process tests that run the real `mostro-cli` against the mailbox on a loopback relay):
  - JSON: the whole flow as exactly one line, exit 0, `--verbose` prints only `verbose: message.list ok`, the document conforms to the **checked-in** schema, 7 messages with the bond invoice
    as data and `next_step: pay_bond_invoice`, read stages with a snapshot, **the database byte-identical**, no trade secret, relay address in either stream; no `--yes` and a stdin that fails
    when read change nothing;
  - human: stderr empty, each invoice alone on its line, the next step in words with the command that continues, the refusal reason, oldest first;
  - `--since 9` and `--order` narrow the result; the store is unchanged;
  - bad arguments (`0`, `10081`, `-3`, `soon`) exit 2 with `invalid_arguments` in JSON and one `error:` line in human mode, and **no relay was contacted**;
  - an unknown order is `order_unknown` (exit 1, no relay contact); no identity is `identity_missing` and **no database is created**;
  - `getdm` is rejected as an unknown subcommand (exit 2) in both modes, no relay contacted;
  - a read-only store (unix) is read without complaint.
- **Mutation spot checks** (3): the store opened for writing, the human error line removed, and an invalid window silently defaulted are each caught.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast`, build, MSRV 1.94.0 742 passed and 0 failed (no FAILED line, no compile error).

## Message List, Slice 5: Failure Matrix And Examples: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `e385ec6` (pushed). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (the real binary, each case as JSON, human and `--verbose`): no store and no user (`identity_missing`); a file that is not a database, no `users` table and a null
  mnemonic (`identity_unreadable`); no node key and no relays (`configuration_missing`); a bad node key and a bad relay (`configuration_invalid`). For every case: exit 1, the same code in each mode,
  the JSON document conforms to the checked-in schema with read stages and no snapshot, stdout empty and one `error:` line in human mode, no sentinel, relay address or invoice in either stream, **no
  relay contacted**, the store byte-identical and nothing created in the home or working directory.
- **Relays**: a relay nobody listens on is `relay_connection_failed` with no snapshot; one that never answers is `read_timeout` with a snapshot; neither echoes the address; the store is unchanged.
- **Streams**: a stdout that cannot be written exits 1 with nothing on stdout and no invoice on stderr, in both modes; a stderr that cannot be written changes neither the error exit (2) nor a
  successful read (0) with `--verbose`.
- **Terminal safety**: node text carrying escape and bell characters is shown with them neutralized.
- **Examples**: `contracts/examples.json` (10 cases: a trade's messages, an empty window, the limits with the `trade_keys_limited` warning, the six error codes and the removed `getdm`) conforms to
  the checked-in schema, is reproduced **exactly** by the owned constructors, and 12 contradictory edits (unknown next step or payload kind, numeric index or count, uppercase action, missing flag,
  a write stage, no snapshot, an uncertain read, another command's warning, a store-write code, data on an error) are rejected.
- **Mutation spot checks** (3): a write failure ignored, node text not escaped and a relay address echoed in an error are each caught.
- **Not changed, noted**: a bad `--pow`/`--transport` (or `POW`/`TRANSPORT`) still fails `message list` as `configuration_invalid` because the read shares the write commands' configuration step,
  although it sends nothing; the matrix does not pin that.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 749 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Message List, Slice 6: Docs, Boundaries, Verification: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `f5db27c` (pushed). Lean mode: the only evidence entry for the slice.

- **The small fix** (asked for with this slice): a read sends nothing, so `message list` no longer looks at `--pow`, `--transport`, `POW` or `TRANSPORT`. `cli/write.rs` gained `resolve_reader`
  (configuration, identity, store); `resolve_base` is that plus the request settings, with the same order of checks as before for the write commands. A new process test (a bad value of each
  setting, as a flag and as a variable, still reads) failed first with `configuration_invalid`, exit 1, and passes now; the 47 + 46 + 40 + 35 process tests of the other commands are unchanged.
  Recorded as FR-011 in the spec.
- **Docs**: spec and plan statuses, `quickstart.md` (focused commands, an offline smoke check that was run and matches, the acceptance matrix, the probe procedure), architecture (the message-list
  paragraph, `get_dm` marked admin-only), README and AGENTS lists, `docs/ai-development.md`, and the requirement evidence table in `tests/README.md` (FR-001 to FR-011, SC-001 to SC-006).
- **Boundaries**: every new file is under the 400-line limit with imports at the top (the source-boundary test passes); the transport boundary test still finds no new direct fetch.
- **Open, by design**: SC-006 (no claim about a real node: whether real relays honor one filter with many recipient keys, and their per-relay limit), the MCP DM tools, a cursor for slow flows, peer
  chat as a command of its own, and a smoother solver-key flow for disputes (`getdm` used to save the solver's key; `message list` returns it as data).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 750 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Real-Node Probe Of message list: 2026-11-02

Authorized by the maintainer on 2026-11-02 ("my local node is running you can use it for testing"), with the same two **public relays** as the earlier probe (`wss://mostro-p2p.tech`, `wss://relay.shadowbip.com`),
the only route to the maintainer's local regtest node with a **taker bond**. Two fresh throwaway identities (a maker and a taker) in session temporary directories, the debug build of `develop` at
`c6215d5`; the maintainer's own store was never used; both stores were deleted. The node's version was not queried. The maintainer paid the bond in Polar; no macaroon, certificate or key was shared or read.

- **Two keys in one filter**: the maker created two orders (trade indexes 1 and 2); one `message list --since 10` returned both `new-order` messages, each with its own order id and trade index,
  `keys_read` 2, `keys_not_read` 0, `truncated` false, 2 of 2 relays completed, 2 events. No batching was needed at this size.
- **The point of the spec**: a taker took order 1: `order take-sell` answered `pay_bond_invoice`, and `message list` showed the same message (`pay-bond-invoice`, the parked order `pending`, 1000 sats) with the
  bond invoice as data, so a JSON user can pay it without the legacy command. That bond invoice expired unpaid; a second take (order 2, index 2) gave a fresh one.
- **After the bond was paid**: `message list` showed, in order, `pay-bond-invoice` and `add-invoice` (`next_step: add_invoice`, status `waiting-buyer-invoice`, **6224 sats**, 14 seconds later), the amount to
  invoice (the bond message's 1000 sats is not it, as in the earlier probe). The human output carried the same, with the continuing command in words and the invoice on its own line.
- **The maker's view**: after the take, the maker's `message list` showed the node's `waiting-buyer-invoice` for that order and trade index 2.
- **Cleanup**: both orders were cancelled by the maker (legacy `cancel`); `order info` showed `canceled` for both, and the taker's `message list` then showed a `canceled` message (no `next_step`).
- **Not verified**: many keys (up to the 200 cap) against a real relay, the limit a real relay applies per filter, whether a message can arrive later than the relay's `since` bound (clock skew; none was
  seen in a window of 2 to 10 minutes), and everything the node sends in the later steps of a trade (hold invoice to pay, `fiat-sent`, release).
- **Nothing in the CLI needed to change.**

## Trade Steps (spec 007) Slice 1: Characterize And Script The Node: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `0e63bab` (pushed). Lean mode: the only evidence entry for the slice. No behavior change.

- **Legacy grammar pinned** (`tests/cli_functions.rs`, `trade_step_characterization`): `fiatsent`, `release` and `cancel` take one required UUID (`-o`, long flag `--orderid`, `--order-id` unknown, any case accepted), no `--yes`, and are registered as `order.fiat-sent`, `order.release`, `order.cancel` without JSON.
- **Range `next_trade` condition pinned** (`cli/send_msg.rs`, extracted as `wants_next_trade`, 4 unit tests): the payload is sent when `is_mine` is `Some(_)` (any value, `Some(false)` included: a quirk of the legacy code, to be decided in the plan), the order has a `min` and `max`, and `max - fiat_amount >= min`; a fixed order or an exhausted range sends none. Behavior unchanged.
- **Scripted node** (`tests/common/fake_mostro.rs`: `Traded`, `step_daemon`; `Received.next_trade`): applies the rules of `fiat_sent_action`, `release_action` and `cancel_action_generic` from the v0.20.1 source: the reply to the sender echoes the request id, the notice to the other side does not; fiat-sent only by the buyer on `active`; release only by the seller on `fiat-sent` or `dispute`, answering the seller with `hold-invoice-payment-settled` plus `rate` and the buyer with `released`; cancel as plain `canceled` (pending, waiting), `cooperative-cancel-initiated-by-you/peer`, then `cooperative-cancel-accepted` for the other party, with `order_already_canceled`, `is_not_your_order`, `invalid_pubkey`, `invalid_peer`, `not_allowed_by_status` and `not_found` refusals. Eleven self-tests in `tests/step_process.rs`.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 768 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 2: Outcomes: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `c026687` (pushed). Lean mode: the only evidence entry for the slice. No command exists yet.

- **`outcome/step.rs`**: `StepKind` (command, action, the replies the node answers with, whether the request carries a next trade key), `StepInput`, `StepResult` (`fiat_sent`, `released`, `canceled`, `cancel_requested`, `cancel_accepted`), `StepNext` (`wait_for_release`, `wait_for_peer_cancel`), `StepData` (node, order id, result, next step, trade public key, `next_trade_index` as a decimal string or null) and `from_reply`, which returns the data and the status to record locally (`fiat-sent`, `settled-hold-invoice`, `canceled`, `cooperatively-canceled`; none for a cancel request). A reply is unusable if it is not one of the step's replies, names another order or no order, or carries an unexpected payload. The error, uncertain and warning code lists are the ones of `order add-invoice` minus the order record (no `invalid_order_timestamp`). 11 unit tests: mapping table, decimal index, unusable replies, the schema of each of the three names against real, stale, uncertain, refused and unknown-order outcomes, contradictory edits rejected, and the exact data fields.
- **Stage rules** (`outcome/rules.rs`): `Stages::Step` and `OutcomeRules::step`: as `Update` (success applied, failed or not attempted; the local status is the write) plus, on `uncertain`, persistence `applied` or `failed`, which says whether the reserved range index was recorded.
- **Store** (`store.rs`): `range_has_room(is_mine, min, max, fiat_amount)`, the legacy rule now shared by the legacy path and the new commands, and `Store::range_has_room(order)` (read-only, `false` for an unknown order). 3 store tests in `tests/step_process.rs` (room, exhausted, fixed, unknown, no byte changed, a missing store not created).
- **Mutation spot checks**: `>=` to `>` in the room rule failed the boundary test; dropping the order-id check in `from_reply` failed the unusable-replies test; both restored.
- **Open for slice 3**: after a range `release`/`fiat-sent` the node creates the child order at the reserved trade index, but the CLI no longer waits for it (spec), so no local row exists for it and `order cancel` on the child would be `order_unknown` until something saves it. The plan for slice 3 must pick: save a minimal row for the child at publication time, or let a later command create it. Decision needed from the maintainer.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 780 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 3: The Operation: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `320e85e` (pushed). Lean mode: the only evidence entry for the slice. No command exists yet.

- **`commands/order/step.rs`** (`order_step`, 161 lines): looks up the order's stored trade key (`order_unknown` when there is no row or key, `store_unwritable` when the store cannot be read), publishes one signed request with the order id, no payload, no trade index and no next trade key, selects the reply by the echoed request id (the node echoes it in every reply to the sender, from the source; no order-id fallback, so a reply without it is ignored and the wait times out), maps it with `StepData::from_reply`, then updates the local status. A refusal is `error` with the node's reason; silence, an unusable reply and a relay read failure after publishing are `uncertain`, never retried; a failed status update is `ok` with `local_status_stale`.
- **New warning `range_not_continued`** (`WarningCode`, covered by the contract test of codes): added to an accepted `fiat-sent` or `release` when `Store::range_has_room` is true, because without a next trade key the node ends the range (spec 008 continues it). Nothing else changes for a range order.
- **14 operation tests** (`tests/step_process.rs`, `mod operation`) against the step daemon on a loopback relay: each accepted reply with its saved status (`fiat-sent`, `settled-hold-invoice`, `canceled`, `cooperatively-canceled`; a cooperative request leaves the database byte-identical); the request the node saw (signed by the order's trade key under the identity, one publication, no index, no next trade key); six refusals with the node's reason; silence for all three steps; five unusable-reply cases (an action the step does not allow times out, an allowed one with a wrong id or payload is `reply_malformed`); decoys (stale request id, other author, other addressee, no request id); a forced status-update failure; unknown and keyless orders send nothing; required work stops every step before anything is sent; the range warning for `fiat-sent` and `release` and its absence for a fixed order, an exhausted range and a cancel; the trade index never changes.
- **Mutation spot checks**: always warning on range failed the "no warning" test; skipping the status update failed six tests; both restored.
- **Flake seen once, unrelated**: `read_failures_are_stable_in_every_mode_without_retries` (`tests/order_list_process.rs`) failed in one full run expecting `relay_connection_failed` and getting `relay_read_failed` (a race between a relay that answers then refuses), passed three times alone and in the rerun of the whole suite. Not touched by this slice; noted for a later look.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 794 passed and 0 failed on the clean rerun (the first full run had the flake above), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 4a: The Commands Replace The Legacy Ones: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `75e99a7` (pushed). Lean mode: the only evidence entry for the slice. Binary scenarios are slice 4b.

- **Grammar**: `order fiat-sent`, `order release`, `order cancel`, each with `--order UUID` (a UUID in any case) and `--yes`. `fiatsent`, `release`, `cancel`, `-o`, `--orderid` and `--order-id` are gone with no alias (`trade_step_grammar` replaces the slice 1 characterization, 4 tests). Three registry entries (`JsonSupport::Write`) replace the legacy ones; the registry, machine-command, discovery and old-command tests list them.
- **Dispatch** (`cli/step.rs`, `cli/migrated.rs`): the confirmation rule of the other write commands (JSON needs `--yes`, never prompts; human mode previews and asks, the release preview says it cannot be undone), configuration, identity and a writable store, the order's key for the preview, then `order_step`. In human mode two warnings become fixed stderr lines (`range_not_continued`, `local_status_stale`) because the human result renders data only; the stale-status warning keeps its frozen invoice-worded message in JSON (spec 005 pins it).
- **Human output** (`parse_human/step.rs`, 3 unit tests): a title per result, the order id and trade key, the next step (`wait for the seller`, the command the other party runs to confirm a cooperative cancel).
- **Schemas**: command-scoped for each name (`StepData` with `OutcomeRules::step`), fixtures `order-fiat-sent-v1.json`, `order-release-v1.json`, `order-cancel-v1.json`, regenerated discovery (the other fixtures are unchanged), a drift test for the three and the machine-command list.
- **Legacy removed**: the three `Commands` variants and their `legacy.rs` arm; `cli/send_msg.rs` shrank from 215 to 92 lines and now serves only `dispute` and the admin commands (the next-trade payload, the child-order wait and its `fetch_events` are gone; the frozen transport-call list lost `cli/send_msg.rs`). The schema unit tests and the legacy-JSON tests use `rate` as their example of an unmigrated command.
- **Docs**: README (flow steps, command list, JSON list, the range note, restore notes), command reference, architecture.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 795 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 4b: Binary Scenarios: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `d3cafd4` (pushed). Lean mode: the only evidence entry for the slice. The scenarios run the real `mostro-cli` binary in a sandbox `HOME`/`MCLI_DIR` against the scripted node on a loopback relay (`mod operation`, second half, in `tests/step_process.rs`).

- **Success in JSON, per step** (`fiat-sent` as buyer, `release` as seller of a `fiat-sent` order, `cancel` as a cooperative request): exit 0, the only stderr line is `verbose: order.<step> ok`, one document that conforms to the checked-in schema, the right `result` and `next_step`, `next_trade_index` null, the local status (`fiat-sent`, `settled-hold-invoice`, unchanged `active` for a request), one request published, the trade index unchanged, and neither the trade key nor a relay address on any stream.
- **`--yes` rules**: JSON without `--yes` exits 2 with `invalid_arguments`, empty stderr, nothing published and every file in the data directory byte-identical, for all three steps.
- **Prompt**: `y`, blank and `Y\r\n` accept; `n`, `no`, `maybe` and an empty input decline with exit 1 and the fixed declined line, publishing nothing; a closed stdin is not approval; the preview and the result titles are on stdout and the question on stderr. The release preview says it cannot be undone.
- **Unknown order**: both modes fail before any question with `order_unknown` and publish nothing.
- **Refusal** (the seller saying fiat was sent): `node_refused` with reason `invalid_pubkey` in JSON and as `error: ... (reason: invalid_pubkey)` in human mode; the data directory is unchanged and each run publishes one request.
- **Silence**: after the production reply budget, JSON gives `uncertain` / `reply_timeout` with persistence `not_attempted`, and human mode prints the release-specific advice; the request is never retried.
- **Warnings**: a range order with room left gives exit 0 with `range_not_continued` in JSON and a fixed stderr line in human mode; a frozen order table gives exit 0 with `local_status_stale` and persistence `failed` in JSON and a stderr line in human mode.
- **Cooperative cancel from the binary**: confirming the peer's request gives `cancel_accepted` and the local status `cooperatively-canceled`.
- **Mutation spot checks**: bypassing the `--yes` rule failed the no-`--yes` test; dropping the human warnings failed the range and stale-status tests; both restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 805 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 5: Failure Matrix And Examples: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `009d78b` (pushed). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (`tests/step_process.rs`, binary, all three steps x JSON, human and `--verbose`): no store, no user, not a database, no users table and a null mnemonic (identity), missing and invalid node key and relays, invalid `--pow` and `--transport` (configuration), a read-only store (`store_unwritable`, skipped when the file mode does not bind the user), a node on protocol 1 (`protocol_unsupported`), a node expecting more work (`pow_required`; an explicit transport skips the protocol check and enough work passes), a relay that rejects writes (`request_rejected`) and an unreachable relay (`relay_connection_failed`). Each case: exit 1, the stable code, `not_attempted` / `unknown` / `not_attempted`, nothing published, the data directory byte-identical (except the read-only case), nothing created in `HOME` or the working directory, no sentinel or relay address on any stream, and a JSON document that conforms to the checked-in schema.
- **Invalid input**: 18 invocations (a bad id, a missing id, a stray `--invoice`, the old `--order-id`, `-o`, `--orderid`, per step) and the three removed spellings give exit 2, `invalid_arguments`, one document, `not_attempted`, an empty stderr and no echo of the input; nothing is published.
- **Streams**: an unwritable stdout after the step exits 1 with one fixed line per mode (the request was published all the same); an unwritable stderr changes neither the exit nor the document; no trade-key or identity secret reaches either stream on a successful run.
- **Correlation**: a reply without the request id (the notice meant for the other party) is never taken for the answer; the mutation that also matches on the order id failed this test and was restored. Bypassing the writable-store check failed the read-only test; restored.
- **Examples** (`specs/007-trade-steps-json/contracts/examples.json`, 16 cases, written once from the owned constructors): success per step and result, the range and stale-status warnings, three node refusals, two uncertain outcomes, unknown order, declined, `--yes` required and the removed spelling. Tests (`integration_tests.rs`): every example conforms to its checked-in schema with the right exit code and data exactly on success; the examples are reproduced byte for byte by the constructors; 17 invalid variants (a bad result, next step, id, key, index, status, stage and warning) do not conform.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 818 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Trade Steps (spec 007) Slice 6: Docs, Boundaries, Verification: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `cf10bd7` (pushed). Lean mode: the only evidence entry for the slice. Documentation only.

- **Docs**: `contracts/cli.md`, `data-model.md` and `quickstart.md` for the spec (focused commands, an offline smoke check that was run and matches, the acceptance matrix, the regtest probe procedure for a whole trade, the gates), the requirement-evidence table in `tests/README.md` (FR-001 to FR-010, SC-001 to SC-006), status lines in the spec, plan, tasks, `AGENTS.md` and `docs/ai-development.md`. The command reference, README and architecture were migrated in slice 4a.
- **Boundaries**: every new file is under the 400-line production limit with imports at the top and the source-boundary test passes; the transport boundary test shows no new direct send, fetch or subscribe (the frozen list lost `cli/send_msg.rs`).
- **Open, by design**: SC-006 (no claim about a real daemon; the probe needs a paying seller on the maintainer's node), spec 008 (range children: the next trade key, the reserved index and a local row for the child order; until then `fiat-sent` and `release` warn with `range_not_continued`), the MCP `fiat_sent`, `release` and `cancel` tools, the dispute commands and `rate`. CI (Windows included) is checked after the push. One unrelated flaky test was seen once (slice 3 entry).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 818 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Real-Node Probe Of The Trade Steps: 2026-11-02

Authorized by the maintainer on 2026-11-02 ("go testing on my local node, it's still running"), with the same two **public relays** as the earlier probes (`wss://mostro-p2p.tech`, `wss://relay.shadowbip.com`), the only route to the maintainer's local regtest node with a **taker bond**. Two fresh throwaway identities (a seller who made the orders and a buyer) in session temporary directories, the debug build of `develop` at `1a93197`; the maintainer's own store was never used; both stores were deleted. The node's version was not queried. The maintainer paid the invoices in Polar (the taker bond, the buyer's payout invoice for the amount the node asked, and the seller's hold invoice, twice); no macaroon, certificate or key was shared or read.

- **A whole trade** (order `52bb49b4…`, fixed sell order of 15 USD): `order new`, `order take-sell` (bond), `order add-invoice` (18972 sats, `waiting_seller_payment`), the hold invoice paid (20146 sats), both sides active. Before the trade could proceed: the seller's `fiat-sent` was refused with `invalid_pubkey` and the seller's `release` with `not_allowed_by_status`, each sending one request and changing nothing locally. `order fiat-sent` by the buyer: `ok`, `fiat_sent`, `wait_for_release`, persistence `applied`; the seller received `fiat-sent-ok`. A second `fiat-sent` was refused with **`invalid_order_status`** (the scripted node says `not_allowed_by_status`: the CLI passes the reason through, so no change is needed); the buyer's `release` with `invalid_peer`. `order release` by the seller: `ok`, `released`, applied; the seller received **`hold-invoice-payment-settled`** (the request id echoed, as the correlation assumes) followed by `rate`, the buyer `released`, `rate` and `purchase-completed`; `order info` showed `success` on both sides. A second release: `not_allowed_by_status`.
- **Cancel without payments**: a pending order canceled by its maker (`canceled`, applied; the node shows `canceled`; a second cancel `order_already_canceled`); a taker who took an order and never paid the bond canceled it (`canceled`, applied; the order stays `pending` for the maker); a stranger without a local row got `order_unknown` before anything was sent.
- **Cooperative cancel of an active trade** (order `f8b6ffa5…`, a second full cycle): the buyer's `order cancel`: `ok`, `cancel_requested`, `wait_for_peer_cancel`, persistence `not_attempted` (both sides still `active`); the same party asking again: `invalid_pubkey`; the seller received `cooperative-cancel-initiated-by-peer`; the seller's `order cancel`: `ok`, `cancel_accepted`, applied; the node showed `cooperatively-canceled` on both sides; the buyer received `cooperative-cancel-initiated-by-you` then `cooperative-cancel-accepted`; a cancel afterwards: `order_already_canceled`.
- **Not verified**: a range order (the `range_not_continued` warning and what the node does without the next trade key), `uncertain` against a stopped node, `fiat-sent` and `release` on a disputed order, the `released` action reaching a seller (only `hold-invoice-payment-settled` was seen), cooperative cancel from a `fiat-sent` order, other node versions.
- **Nothing in the CLI needed to change.**

## Range Children (spec 008) Slice 1: The Maker Marker: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `7880630` (pushed). Lean mode: the only evidence entry for the slice.

- **The flaw, found while writing the spec**: `Store::save_order` wrote `is_mine = 1` for every order, so the rows of `order take-*` looked like the user's own and the `range_not_continued` warning of spec 007 also appeared for the taker of a range order (the legacy `Some(_)` test had the same flaw). Two tests were written first and failed for that reason: `no_range_warning_for_the_taker_of_a_range_order` (the warning appeared) and `the_maker_marker_is_exact_and_only_the_makers_range_has_room` (a taker's range reported room).
- **The fix**: `store::Role` (`Maker`, `Taker`), `Store::save_order_as` (`save_order` stays the maker's), the marker written on insert and on update; `Sent` carries the role (`order new` maker, `order take-*` taker); `range_has_room` requires `is_mine = true`. Rows written before this change keep their marker (the spec 008 clarification: no migration).
- **Evidence**: the two store tests (exact marker for both roles, `save_order` is the maker's, a second save updates the role), the taker warning test, and one new assertion each in the `order new` and `order take-sell` process tests that the saved row has the right marker. The spec 007 texts (spec, data model, command reference) now say "an order you made".
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 821 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Range Children (spec 008) Slice 2: The Scripted Node Creates Children: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `7ce2f8a` (pushed). Lean mode: the only evidence entry for the slice. No production change.

- **Scripted node** (`tests/common/fake_mostro.rs`: `Traded::ranged`, `Traded::active_buy`, `child_id`, the range-aware `step_daemon`): a range order has bounds and the slice taken. `fiat-sent` keeps the next trade key only on a range order. `release` creates the child as `get_child_order` and `handle_child_order` of the v0.20.1 source do: only with a key (a sell order's `release` carries it, a buy order's comes from the stored `fiat-sent` key), only when `max - taken` still reaches the minimum (equal: a fixed order of that amount; larger: a range with the new maximum, nothing taken), and announces it with a `new-order` message addressed to the **new** key, whose id is the child's, whose trade index is the key's index, whose payload is the child order (`pending`) and which carries the request id of the triggering request (the seller's release, for a buy order).
- **Self-tests** (`mod children`, 5): the sell order's child with the right id, index, request id and remaining bounds, and the registered second trade; the equal remainder becoming a fixed order; no child without a key, below the minimum or on a fixed order; the buy order's key kept at `fiat-sent` and its child created at the peer's `release`; no key kept on a fixed order.
- **A finding for slice 3**: the maker's local row is written at `order new` and never refreshed, so its `fiat_amount` is the creation value (0 for a range order), not the slice taken. The room test `max - fiat_amount >= min` on that row is therefore always true for a range order the user made, so the key is sent even when the taken slice leaves nothing. This is harmless: the node ignores a key when no child is due, and the only cost is one spent trade index. `order sync` finds no child in that case and says so.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 826 passed and 0 failed (no FAILED line, no compile error; the first run caught the older `fiat_sent_keeps_the_next_trade_key` self-test, which now seeds a range order), build, MSRV 1.94.0.

## Range Children (spec 008) Slice 3: Key And Index In The Steps: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `8a5c1b9` (pushed). Lean mode: the only evidence entry for the slice.

- **Behavior** (`commands/order/step.rs`, `outcome/step.rs`, `store.rs`): `Store::maker_range` returns the kind of a range order the user made with room left (`None` for a taker, a fixed order, an exhausted range or an unknown order). `StepKind::sends_key(sells)`: `release` of a sell order and `fiat-sent` of a buy order. When it applies, the next trade (`Store::next_trade`) is reserved right before publishing and sent as `NextTrade(key, index)`; the index is then spent (`Store::advance`) once the request was published and the node did not provably refuse it: a refusal spends nothing, silence, an unusable reply and a relay read failure spend it (`uncertain` with persistence `applied`), an accepted step spends it (persistence `applied`; `failed` with the new warning `trade_index_not_recorded` if it could not be recorded). Nothing that never left the machine spends anything.
- **Data**: `data.next_trade_index` is the reserved index; `next_step` is `sync_child_order` after a `release` that sent a key, and stays `wait_for_release` after a buyer's `fiat-sent`. The human text of both points at `order sync`. The warning `range_not_continued` of spec 007 is **removed** (the key is now sent whenever it can be); `trade_index_not_recorded` replaces it in the schemas. The spec 007 and 008 documents, the command reference, README, architecture and quickstart are amended; fixtures and `examples.json` regenerated (17 cases: `fiat_sent_range_key_sent` and `release_range_continued` replace the old range warning case).
- **Tests**: `the_makers_own_step_on_a_range_order_carries_a_key_and_spends_one_index` (a seller's release and a buyer's fiat-sent; key at the next index, index recorded), `no_key_for_a_taker_a_fixed_order_an_exhausted_range_the_wrong_step_or_a_cancel` (five cases, nothing sent, nothing spent), `the_index_is_spent_when_the_node_may_hold_the_key_and_kept_when_it_refused` (refusal, silence, unusable reply, work required), `a_trade_index_that_cannot_be_recorded_is_a_warning_...` (a frozen users table), two binary tests (JSON and human, the schema, no secret) and unit tests for `sends_key`, the next step and the human text.
- **Mutation spot checks**: swapping the roles in `sends_key` failed the key test; not spending the index on silence failed the index policy test; both restored.
- **Known and accepted**: the maker's local `fiat_amount` is the creation value, so the room test cannot see the taken slice; the key may be sent when the slice leaves no room, and one index is then spent (slice 2 finding).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 830 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Range Children (spec 008) Slice 4: order sync: 2026-11-02

Native Linux, rustc 1.97.0, on `develop` after `2512065` (pushed). Lean mode: the only evidence entry for the slice.

- **Command** (`order sync [--since MINUTES]`, canonical `order.sync`, `JsonSupport::Read`): reads the node's messages for the derived trade keys through `msg::inbox::read_inbox` (as `message list`), keeps `new-order` messages whose payload order has the message's id, a kind, a status and a known key index, and whose id the store lacks, newest announcement per id, oldest first, and saves each with `Store::adopt_order` (the user's own order, the key of its index, the index recorded in the same transaction, never replacing a row). It publishes nothing, takes no `--yes` and asks nothing (amended in the spec: the rule exists for irreversible publications). Window default and maximum 10080 minutes.
- **Outcomes** (`outcome/sync.rs`): `SyncInput`, `SyncData` (`adopted` with the order record, trade index and key; `messages_read`, `messages_ignored`, `keys_read`, `keys_not_read`, `truncated`), `Stages::Sync` / `OutcomeRules::sync` / `ExecutionMeta::sync` (publication and acceptance not applicable; persistence `applied`, `not_attempted`, or `failed` when a save failed after others succeeded, which is `uncertain` / `order_not_saved` with the rows already saved). Schema `docs/schemas/order-sync-v1.json`, drift test, regenerated discovery, registry, grammar and machine-command lists.
- **Store**: `Store::known_orders`, `Store::adopt_order`, and the order-writing statement moved to `store/save.rs` with a "keep or replace" conflict rule (two fixed SQL statements); `save_order` is unchanged. `cli/commands.rs` shed its argument structs into `cli/command_args.rs` to stay under the 400-line limit.
- **Operation tests** (`tests/sync_process.rs`, scripted mailbox on a loopback relay, 9): an announced unknown order gets a row (terms, `is_mine`, the key of index 3, usable as a range maker); a second run saves nothing and changes no byte; a known order is never replaced; the newest announcement wins and several orders come oldest first; other actions, messages signed by someone else, addressed to a key the user lacks, with a mismatched id, without an order or outside the window are ignored; nothing to add writes nothing; a failing save stops the run and keeps the rows already saved (`uncertain`); a missing store and an unreachable relay change nothing; and an adopted child is cancelled at once through the step operation against the scripted node.
- **Binary tests** (6): JSON saves the order, conforms to the checked-in schema, shows no key or relay, and a second run saves nothing; human output lists the order and the next command; an invalid or unknown option is `invalid_arguments` (exit 2) with nothing changed; four configuration failures, a read-only store and a missing store are stable and leave no trace; an unwritable stdout (one fixed line per mode, the row already saved) and an unwritable stderr.
- **Mutation spot checks**: accepting any action as an announcement failed the filter test (restored). Dropping the known-orders filter alone left every test green: it is redundant with `Existing::Keep` for correctness (the save returns `false` for a known id), and the two guards are kept as defense in depth.
- **Docs**: README, command reference, architecture and the spec amendment; fixtures and the examples file are slice 5.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 850 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Range Children (spec 008) Slice 5: Failure Matrix And Examples: 2026-11-03

Native Linux, rustc 1.97.0, on `develop` after `4a71d3d` (pushed; it also carries the maintainer's commit that moved the JSON interface book to its own repository). Lean mode: the only evidence entry for the slice.

- **Failure matrix** (`tests/sync_process.rs`, binary, JSON, human and `--verbose`, joined with the slice 4 cases): no store, no user, not a database, no users table and a null mnemonic (identity), missing and invalid node key and relays (configuration), a read-only store (`store_unwritable`, skipped for a user the file mode does not bind), an unreachable relay (`relay_connection_failed`), an invalid window or an unknown option (`invalid_arguments`, exit 2), and `--pow`/`--transport`/`POW`/`TRANSPORT` bad values not concerning a sync (exit 0). Each failure: exit 1 with the stable code, meta `not_applicable` x3 before the relay and `not_attempted` persistence after it, no data, a document that conforms to the checked-in schema, the data directory byte-identical, nothing created in `HOME` or the working directory, no sentinel or relay address on any stream.
- **Streams and secrets**: an unwritable stdout exits 1 with one fixed line per mode after the row was saved; an unwritable stderr changes neither the exit nor the document; neither the trade key nor the identity key reaches a stream on a successful run.
- **Examples** (`specs/008-range-children/contracts/examples.json`, 10 cases, written once from the owned constructors): a child order added, nothing to add, more messages and keys than were read (with `trade_keys_limited`), one order saved and one not (`uncertain` / `order_not_saved` with the saved row), identity missing, store unwritable, store unreadable after the keys, relays unreachable, read timeout (with its snapshot) and a window out of range. Tests (`integration_tests.rs`): every example conforms to `order-sync-v1.json` with the right exit code and data exactly where expected; the constructors reproduce the file byte for byte; 15 invalid variants (a numeric index, an uppercase key, an order without an id, a numeric count, a missing flag, a published request, rows saved without persistence, no snapshot, a failed write on a success, a warning of another command, a plain error status, an error-only code, persistence that did not fail, a node refusal, data on an error) do not conform.
- **Mutation spot check**: making `order sync` read the request settings (`resolve_base`) failed the settings test; restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 857 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Range Children (spec 008) Slice 6: Docs, Boundaries, Verification: 2026-11-03

Native Linux, rustc 1.97.0, on `develop` after `8229269` (pushed). Lean mode: the only evidence entry for the slice. Documentation only.

- **Docs**: `contracts/cli.md`, `data-model.md` and `quickstart.md` for the spec (focused commands, an offline smoke check that was run and matches, the acceptance matrix, the regtest probe procedure for a sell and a buy range order, the taker case and an exhausted slice, the gates), the requirement-evidence table in `tests/README.md` (FR-001 to FR-007, SC-001 to SC-005), status lines in the spec, plan, tasks, `AGENTS.md` and `docs/ai-development.md`. The command reference, README and architecture were updated in slice 4, and the spec 007 documents in slice 3.
- **Boundaries**: every file is under the 400-line production limit with imports at the top (`cli/commands.rs` and `store.rs` were split in slice 4 to stay under it) and the source-boundary tests pass; no new direct send, fetch or subscribe.
- **Open, by design**: SC-005 (no claim about a real node; the probe needs a paying seller and taker on the maintainer's node, and the range buy case in particular), the accepted limits (the maker's local `fiat_amount` is the creation value so a key may be sent when the taken slice leaves no room, one index spent; rows saved before the marker was exact keep the old one), the MCP migration, `rate`, the dispute commands and the solver-key flow. CI (Windows included) is checked after the push.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 857 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0.

## Real-Node Probe Of Range Children: 2026-11-03

Authorized by the maintainer on 2026-11-03 ("push and range probe"), with the same two **public relays** as the earlier probes (`wss://mostro-p2p.tech`, `wss://relay.shadowbip.com`), the only route to the maintainer's local regtest node with a **taker bond**. Two fresh throwaway identities (a seller who made the range order and a buyer who took it) in session temporary directories, the debug build of `develop` at `7590364`; the maintainer's own store was never used; both stores were deleted. The node's version was not queried. The maintainer paid the invoices in Polar (the taker bond, the buyer's payout invoice for 62385 sats and the seller's hold invoice of 66243 sats); no macaroon, certificate or key was shared or read. (One hold invoice was first copied with a typo into the chat, by the assistant; the corrected invoice was copied from the program's output.)

- **The order**: `order new --side sell --fiat-amount 50 --fiat-amount-max 150` (a range of 50 to 150 USD); the buyer took 50 with `order take-sell --amount 50`; the bond, the buyer's invoice (`waiting_seller_payment`) and the hold invoice followed; both sides showed `active` with 50 USD taken.
- **The buyer's `fiat-sent` on the order they took**: `ok`, `fiat_sent`, `next_trade_index` null; the buyer's trade index stayed at 1, no key was sent, and no warning appeared (the taker marker works on a real run).
- **The seller-maker's `release`**: `ok`, `released`, **`next_step` `sync_child_order`, `next_trade_index` "2"**, persistence `applied`; the maker's index went from 1 to 2. Both sides then showed `success`. The node accepted the request carrying the key and created the child order for it.
- **What the node sent**: the maker received, after `fiat-sent-ok`, a **`new-order`** message for a new order id at **trade index 2** with payload range **50 to 100** (150 minus the 50 taken), status `pending`, and then `hold-invoice-payment-settled` and `rate`. This is what the v0.20.1 source predicted.
- **Before the sync**: `order cancel` on the child was `order_unknown`, as designed. **`order sync`** (maker): `ok`, persistence `applied`, publication `not_applicable`, one order added (the child, index 2, `pending`, 50 to 100, `sell`), 9 messages read and 8 ignored, 2 keys read. A second run: `ok`, persistence `not_attempted`, nothing added. The buyer's `order sync`: nothing to add. The human output of a run with nothing due: "Nothing to add".
- **The child is usable**: `order info` showed it `pending` 50 to 100 on the node; the maker's `order cancel` on it: `ok`, `canceled`, applied; the node showed `canceled`.
- **Not verified**: a **buy** range order (the maker's `fiat-sent` key kept by the node and the child announced at the peer's release), a slice that leaves less than the minimum (key ignored, one index spent), a second trade on the child (its own key at the next index), an `uncertain` run on a stopped node, other node versions.
- **Nothing in the CLI needed to change.**


## Range Children (spec 008) Slice 7: Reserve The Trade Index Before Publishing: 2026-11-05

Native Linux, rustc 1.97.0, on `pr/13-range-children` after `4ba6508` (local commit, not pushed). Lean mode: the only evidence entry for the slice. The fix for a confirmed P1 review finding on PR #232 (`src/commands/order/step.rs` at `a364055`).

- **The defect**: `order new`, `order take-*` and the range continuation of `order release`/`order fiat-sent` read the next trade index before publishing (`Store::next_trade`, read-only) and recorded it only after the exchange (`Store::advance`, `save_order`). A run killed in that window (Ctrl-C, a kill, a crash; up to the 15 s reply budget) left the index unrecorded, and two runs at once picked the same index, since nothing held a lock. The node never validates or records a `NextTrade` index, so after a continuation the next order or continuation silently reused the child's key, and `order sync` could not see the child (it derives keys only up to the recorded index). For `order new` and the takes the node had recorded the index, so every later request was refused with `invalid_trade_index` and the orphaned order stayed invisible. The released legacy CLI spent a continuation index before sending, so this was a regression for continuations, and it contradicted constitution VI.
- **Failing first** (on `4ba6508`, production code unchanged): `a_continuation_killed_after_publishing_keeps_its_index_and_sync_adopts_the_child` (the real binary is killed once the scripted node received the release; the recorded index stayed 1, expected 2), `two_continuations_at_once_never_share_an_index` (both releases carried index 2), `a_run_killed_after_publishing_never_reuses_its_key_and_sync_adopts_the_order` (the index was `NULL` after the kill, expected 1) and `two_runs_at_once_never_share_an_index` (two `order new --yes` processes both sent index 1). All four pass after the change. The kill uses `harness::kill_once_published` (SIGKILL on Unix, TerminateProcess on Windows).
- **The fix** (`src/store/reserve.rs`): `Store::reserve_trade` records the next index (`max(last, 0) + 1`, at most `LAST_TRADE_INDEX` = 2^31 - 1, the last normal BIP32 address) in one `BEGIN IMMEDIATE` transaction committed before anything is published, then derives the key from the mnemonic read in it. Never `RETURNING` through `fetch_one`/`fetch_optional`, which drop a failed commit. `Store::release` gives the index back by compare-and-swap to the exact previous value (`NULL` included) while it is still the recorded one; otherwise the index is a harmless gap. `order new` and `order take-*` reserve as their first statement; the CLI peeks (`Store::next_trade`) only for the human preview, and the result reports the index and key actually used. The continuation reserves only for the maker's step on a range with room. A request that never left or a `cant-do` refusal gives the index back; silence, an unusable reply, an accepted request and a failed save keep it spent (`meta.persistence` `applied`; `failed` only with `order_not_saved` or `local_status_stale`). `Store::advance` and `Sent::spend_index` are gone; the warning `trade_index_not_recorded` is removed and the schemas regenerated (discovery, fiat-sent, release, cancel). Trade keys have one derivation, `identity::trade_keys_from_mnemonic`. The legacy `getlasttradeindex` handler and `util::save_order` only raise the index (`util::raise_trade_index`); the former prints the local index when it stays above the node's.
- **A design change found by measurement**: the reviewed design derived the key inside the transaction. With the derivation (about 18 ms in the test profile) under the write lock, 32 concurrent reservations took 3.3 to 5.4 s and one run in three overran the 5 s busy timeout. Deriving right after the commit, the same test takes 2.6 to 2.9 s and passed every run; a derivation that fails after the commit gives the index back.
- **Tests added or changed**: store: recorded before returning, never below 1, the last address reserved and nothing past it; the reserved key is the previewed one; 32 concurrent reservations on separate handles get 1..=32 with distinct keys; a reader holding its lock makes the commit fail with `Unwritable` and nothing recorded (about 5 s); the give-back restores `NULL` exactly, only for the newest reservation, and only once; a frozen users table or a missing user row reserves nothing and changes no byte. Operation: an order whose row cannot be saved (an `orders` insert trigger) keeps index 1; a missing user row sends nothing (`identity_missing`). `an_index_that_cannot_be_reserved_stops_the_step_before_anything_is_sent` replaces the warning test. Legacy: a node reply of 3 keeps a local 7; `save_order` with an older index keeps the newer one. Source boundary: no `.next_trade(` under `src/commands`, and `reserve_trade(` only in `new.rs`, `take.rs` and `step.rs`. The unchanged index-policy test of the steps (`the_index_is_spent_...`) and the refusal and silence value tests still pass.
- **Byte snapshots**: failures after the reservation (work, protocol, a rejected or unreachable relay, refusals) now compare the order rows and the raw `last_trade_index` instead of the file bytes: the give-back restores the values, but SQLite's change counter moves (maintainer decision). Arguments, configuration, identity, no `--yes`, a declined prompt and an unwritable store keep their byte snapshots, which shows that the reservation comes after the prompt.
- **Mutation spot checks**: a deferred `begin()` failed the 32-way test at once; ignoring the commit error failed the blocked-commit test; dropping the step's refusal give-back failed `the_index_is_spent_...`. All were restored.
- **Docs**: specs 003, 004, 007 and 008 (SC-006 added, tasks slice 7), the architecture, the command reference, README and `tests/README.md`; every amendment is marked "amended 2026-11-05 by spec 008". The external JSON interface book still lists `trade_index_not_recorded` (follow-up in its own repository).
- **Not verified**: a real node; the CI Windows runner (after the push). The 32-way test leaves about 2 s of the 5 s busy timeout here, and a slow runner narrows that margin.
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 892 passed and 0 failed (no FAILED line, no compile error), build, MSRV 1.94.0 build `--all-features`.

### Slice 7 Review Follow-Up: 2026-11-05

Native Linux, on `pr/13-range-children` after `df27fa5` (local commit, not pushed). Two findings of the review of slice 7, both confirmed.

- **A stale-index refusal kept the CLI behind the node**: `Sent::refused` gave the index back on every `cant-do`, on the premise that the node refuses before it records anything. The v0.20.2 source (`/disco/dev/mostro`, `v0.20.2-2-g8111f5e`) shows otherwise: `take_sell_action` and `take_buy_action` call `update_user_trade_index` before they apply the take, and a lost compare-and-swap afterwards answers `not_allowed_by_status` (`take_sell.rs` `update_order_status` and `util.rs` `show_hold_invoice` for a take-sell, `util.rs` `show_cashu_escrow_request` for both takes); `check_trade_index` also creates an unknown identity's user row with the request's index before the action runs. The give-back then left the local index below the node's, and every later `order new` or take carried that index again, was refused with `invalid_trade_index` and gave it back again. **Failing first**: `an_index_the_node_recorded_before_refusing_costs_one_stale_refusal_not_every_run` (a scripted node records a take's index and refuses it with `not_allowed_by_status`, then checks every index): the third run was refused with `invalid_trade_index` again instead of sending index 2. **The fix** (`src/commands/order/write.rs`): a refusal for `invalid_trade_index` keeps its index, `meta.persistence` `applied` (`Stages::Write` gains `published/rejected/applied`; the order-new, take-sell, take-buy and discovery schemas regenerated). Every other refusal still gives the index back (maintainer decision 2), so after a lost take the next request is refused once and the one after it goes through. The step path is unchanged: mostrod's `fiat_sent` and `release` refuse before they store the next trade, and `invalid_trade_index` applies only to `new-order` and the takes. Changed tests: the process refusal tests of `order new` and `take-sell` now start two indexes behind the node and show 1, 2, then an accepted 3 (they showed 1, 1 before); the operation refusal tests use a reason that is given back and check the stale case apart; the negative schema variant `persistence: applied` on `node_refused` became `failed`.
- **The 32-way concurrency test depended on disk sync speed**: reproduced with `strace -f --seccomp-bpf -e inject=fsync:delay_exit=25ms` on the test binary: `Unwritable` after 5.74 s (2.72 s without the delay). With 8 reservations it takes 0.26 to 0.39 s, and 2.3 to 2.8 s, 4.2 to 4.4 s and 6.6 to 7.0 s with 25, 50 and 100 ms per `fsync`, all passing (3 runs each). **Mutation check**: a deferred `begin()` fails the 8-way test 10 times out of 10.
- **Not verified**: a real node losing a take race; the CI Windows runner (after the push). The external JSON interface book does not list the new stage combination yet (follow-up in its own repository).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --locked --no-fail-fast` 893 passed and 0 failed, build, MSRV 1.94.0 build `--all-features`.

## Identity Commands (spec 009): identity init And identity show: 2026-11-05

Native Linux, rustc 1.97.0, on `main` at `6cff9b5` (identical to `develop`; uncommitted working tree, no Git mutation). Lean mode: the only
evidence entry for the spec (slices 1-4). No node, relay or regtest: the commands are local; one test uses the scripted node to show that the new
store is usable by `order new`.

- **Why**: every migrated command that signs needs an existing identity and never creates one; only the legacy first run created it (needing
  `MOSTRO_PUBKEY`, `RELAYS`, printing on stdout and connecting to relays). An end-to-end harness driving the CLI through `--output json` needs a
  local, machine-readable way to create and inspect the identity.
- **Failing first**: `tests/identity_process.rs` (14 tests) ran against the unchanged binary: 0 passed, 14 failed, every run exiting 2 with
  "unrecognized subcommand". The reference for "the same store" was characterized on that binary: `getlasttradeindex` with `RELAYS=notarelay`
  creates the store and then stops at the relay list without network access (exit 1): 0700 directory, 0600 file, rollback journal (header bytes
  18-19 = 1, 1), tables `orders` and `users`, one user with a 64-hex key, a 12-word mnemonic and a NULL trade index.
- **The change**: the first-run creation moved unchanged from `db.rs` to `store/create.rs` (`SCHEMA`, `create_private_db_file`, `initialize`;
  `db::connect` keeps its prints) and `store::create_store(dir)` was added for `identity init`: refuse an existing `mcli.db` or a leftover
  `-journal`/`-wal`/`-shm` (`identity_exists`, nothing touched), `ensure_private_dir`, `create_new` 0600, open by file name, `initialize`, close;
  on failure the half-created file is removed (`store_unwritable`, persistence `failed`). `identity::load_identity_from` returns the key and the
  last trade index (NULL is 0; a WAL without its `-shm` file or an index outside `0..=2^31-1` is unreadable). `outcome/identity.rs`
  (`IdentityInitData`, `IdentityShowData`, the error lists), `ErrorCode::IdentityExists`, `Stages::Local`/`OutcomeRules::local`/
  `ExecutionMeta::local`, an integer schema for the index; `commands/identity.rs`, `cli/identity.rs` (through `write::finish`),
  `parse_human/identity.rs`, `Commands::Identity`, two registry entries (`JsonSupport::Read`, as `order sync`). `cli/schema.rs` crossed 400
  production lines with the new schemas, so the Clap input-schema generator moved to `cli/input_schema.rs`. `db.rs` shrank from 704 to 629
  production lines; its frozen cap was lowered to 629, and the stale `OpenOptionsExt` legacy-import exception removed (the import is now at the top
  of `store/create.rs` under `#[cfg(unix)]`). Fixtures `docs/schemas/identity-init-v1.json` and `identity-show-v1.json` and
  `discovery-v1.json` were regenerated with the binary; `specs/009-identity-json/contracts/examples.json` (10 cases) was written once from the
  owned constructors by a temporary printer test, then the printer was removed.
- **A boundary caught by the tests**: the first version called `crate::db::create_store` from `commands/identity.rs`;
  `renderers_operations_and_outcomes_perform_no_io` refused it (operations go through `store` and `identity`), so the creation lives in `store`.
- **Tests**: 14 process tests (the legacy comparison; JSON and human; a second init and leftover companions byte-identical; an uncreatable
  directory and a data path that is a file; two inits at once create one store; show with index 0 and 7; missing, garbage, no user row, NULL
  mnemonic and negative index; options rejected with read stages; root options ignored; unwritable stdout in both modes; `order new --yes`
  against the scripted node with the new store, trade index 1), secret sweeps (mnemonic, its first three words, the identity secret in hex and
  bech32) on every stream; unit tests in `store::create` (5), `commands::identity` (2), `identity` (4), `outcome::identity` (3),
  `parse_human::identity` (1), `cli::schema` (1); fixture drift, 10 examples, 13 invalid variants; registry, grammar and machine lists
  updated.
- **Mutation spot checks**: checking only `mcli.db` (not its companions) failed `init_refuses_a_leftover_companion_file`; accepting any trade
  index failed `show_on_a_damaged_store_reports_it_and_changes_nothing`. Both restored.
- **Not verified**: Windows (CI after the push).
- **Amended after review (2026-11-06)**: A write that fails after the file was created is covered by
  `a_store_that_cannot_be_completed_is_removed_and_failed` (injected fill) and the mapping by `each_creation_result_maps_to_its_outcome_row`;
  `no_data_directory_is_store_unwritable_for_init_and_identity_missing_for_show` covers the missing data directory. `create_store` hands its
  fill step to a private `create_with`; behavior unchanged. Mutations, each restored: removing the `discard` call failed the first test
  (`mcli.db` and `mcli.db-journal` left), `Failed` mapped to `not_attempted` the second, and `init` without a data directory as `failed` or
  `show` as unreadable the third. Gates on `pr/14-identity-json` with these tests: fmt, clippy, `cargo test --locked` 930 passed and 0 failed,
  build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.
- **Gates** (both specs, one working tree): fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 938 passed and 0
  failed (no FAILED line, none ignored; 893 before), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.

## Invoice From Stdin (spec 010): --invoice -: 2026-11-05

Native Linux, rustc 1.97.0, same working tree as the entry above. Lean mode: the only evidence entry for the spec (slices 1-3). Scripted node on a
loopback relay; no real node.

- **Failing first**: `tests/invoice_stdin_process.rs` (5 tests then, 7 after the review) against the code before the change: 1 passed (the
  literal invoice) and 4 failed.
  `-` was taken as the invoice itself: `order add-invoice` refused it as invalid; `order take-sell` **published it to the scripted node**, which
  refused it (`node_refused`, `invalid_invoice`); and without `--yes` the human prompt of `order take-sell` waited on the open stdin until the
  10 s limit. The discovery test failed with no marked option.
- **The change**: `cli/stdin.rs` (`STDIN_VALUE`, `STDIN_LIMIT` = 8192, `STDIN_OPTIONS`, `read_line`, `invoice`): a literal value is kept as it
  is; `-` without `--yes` is `invalid_arguments` before stdin is touched; with `--yes` at most 8193 bytes are read and exactly one non-empty
  UTF-8 line after trimming ASCII whitespace is required. `cli/invoice.rs`, `cli/order_take.rs` and `cli/order_new.rs` call it right after the
  JSON `--yes` rule and before validation, which is unchanged. The `--invoice` help of the four commands names the form, and
  `cli/input_schema.rs` adds `"x-stdin": "-"` to it; `discovery-v1.json` regenerated (the output schemas did not change in this step; the
  review regenerated them, see the amendment below).
- **Tests**: the four commands send exactly the stdin invoice in JSON (the bond payout invoice for `bond add-invoice`); human mode with `--yes`
  trims ` \t...\r\n`; empty, blank, two lines, a line after a blank one, more than 8 KiB and invalid UTF-8 are each `invalid_arguments` (exit 2,
  write-command stages `not_attempted`/`unknown`/`not_attempted`) in both modes for all four commands, nothing sent, the store byte-identical, the
  invoice never on a stream; without `--yes` a held-open stdin is never read in either mode; a literal invoice with the human question still
  works. Unit tests (6) for the reader: trimming, rejections, the 8192-byte limit and that a larger input is not read past 8193 bytes, UTF-8,
  the `--yes` rule with a reader that panics if touched. Harness: `run_with_input` and `run_with_open_stdin`.
- **Mutation spot check**: removing the `--yes` guard failed `without_yes_stdin_is_never_read` ("it waited for stdin"); restored.
- **Amended 2026-11-06 after review**: this entry described the code before the review changes that the same branch carries. A terminal as
  stdin is refused at once with `invalid_arguments`, without a read (`std::io::IsTerminal`); it does not wait. An `invalid_arguments` caused by
  the invoice value itself, literal or from stdin, carries `"reason": "invalid_invoice"` (`(reason: invalid_invoice)` on the human line); `-`
  without `--yes` or with a terminal, and every other argument error, carry none (`StdinRefusal::outcome`, `cli/write.rs::refuse_invoice`).
  The output schemas were regenerated: the four commands allow that reason on `invalid_arguments` and the node's reason on `node_refused`
  only; `order info`, `order cancel`, `order fiat-sent`, `order release` and `order take-buy` allow a reason on `node_refused` only. New
  examples: `invalid_invoice` (003), `sell_invalid_invoice` (004), `order_invalid_invoice` and `bond_invalid_invoice` (005). The process file
  now holds 7 tests (the review added `an_invoice_the_client_refuses_carries_the_reason_invalid_invoice` and
  `the_schemas_tie_the_reason_to_its_code`) and the reader 7 unit tests (it added `a_terminal_as_stdin_fails_at_once_without_being_read`).
  Checks run for this amendment on `pr/15-invoice-stdin` at `2cdc05e` with the spec 010 documents corrected (rustc 1.97.0): the 7 process
  tests and the 7 unit tests pass; dropping `|| terminal` from the guard failed `a_terminal_as_stdin_fails_at_once_without_being_read`
  ("stdin was read"), restored; fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 942 passed and 0
  failed (no FAILED line, none ignored).
- **Not verified**: a real terminal as stdin (the refusal is covered only by the unit test with an injected flag: the harness has no
  pseudo-terminal); Windows (CI after the push).
- **Gates** (both specs, one working tree): fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 938 passed and 0
  failed (no FAILED line, none ignored; 893 before), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.

## Order Mine, Expiry In Seconds, Dispute Open, Order Rate, Bond Flows (specs 011-015): 2026-11-05

Native Linux, rustc 1.97.0, same uncommitted working tree on `main` (`6cff9b5`) as the two entries above. Lean mode: one evidence entry for the
batch. Isolated stores, the scripted node and the scripted mailbox; no real node, relay or regtest.

- **Scripted node** (`tests/common/fake_mostro.rs`): `step_daemon` gains `dispute` (an `active` or `fiat-sent` order, by its buyer or seller;
  `dispute-initiated-by-you` with `Payload::Dispute(id)` to the initiator and `dispute-initiated-by-peer` to the other, both echoing the request
  id; a second dispute is silent, as the internal error of mostrod's `dispute_action`) and `rate-user` (`success`, or `settled-hold-invoice` for
  the seller; `rate-received` with the rating; a side that rated before, or a non-party, gets silence), `cancel` at `waiting-maker-bond` (only
  the maker) and the taker's back-out at `waiting-taker-bond` (the order returns to `pending`), all read from mostrod v0.20.2
  (`src/app/dispute.rs`, `rate_user.rs`, `cancel.rs`, `bond/flow.rs`, `src/app.rs` `manage_errors`). `Received.rating`, `Traded.dispute` and
  `Traded.rated_by`, `dispute_id`, `mail_bond_notice`.
- **Failing first** (unchanged binary): `order_mine_process` 0/6 (no such command), `expires_in_process` 1/4 (the option did not exist; the
  refusal test passed trivially), `dispute_rate_process` 0/10. `bond_flows_process` passed 4/4 at once: the bond flows already held, so spec
  015 is verification only, with no production change.
- **The changes**: `order mine` (`store/rows.rs` `Store::order_rows`, read-only, public key only; `commands/order/mine.rs` matches each key to
  the newest 200 derived indexes, sorts by index then creation time; `outcome/mine.rs`; `cli/mine.rs`; `parse_human/mine.rs`). `--expires-in`
  (`NewArgs.expires_in` with a Clap conflict on `--expiration-days`, `OrderRequest.expires_in`, `InvalidOrder::ExpiresIn` for 1..=31536000 and
  for both at once, `small_order` sends `now + seconds`, the preview line). `dispute open` and `order rate` (`commands/order/held.rs`: the
  request on a held order shared by both, by request id, the reply must name the order; `commands/order/{dispute,rate}.rs`;
  `outcome/{dispute,rate}.rs`; `cli/held.rs`; `parse_human/held.rs`); the trade steps now reuse its refusal and unusable-reply outcomes. The
  legacy `dispute` and `rate` were removed with no alias, with `cli/rate_user.rs`, `cli/send_msg.rs` (it served only `dispute` by then) and
  `util::run_simple_order_msg`. Grammar: `Commands::Dispute { Open }`, `OrderCommands::{Mine, Rate}`; `NewArgs`, `TakeSellArgs` and
  `TakeBuyArgs` moved to `cli/command_args.rs` to keep `commands.rs` under the limit. Registry: `order.mine` (read), `order.rate` and
  `dispute.open` (write; canonical names unchanged from the legacy entries). Schemas `order-mine-v1.json`, `dispute-open-v1.json`,
  `order-rate-v1.json` and `discovery-v1.json` regenerated with the binary; examples (6, 6 and 4 cases) written once from the owned constructors
  by a temporary printer test, then the printer was removed.
- **Tests**: the 24 process tests named in `tests/README.md`; unit tests in `outcome::{mine,dispute,rate,new}` and `cli::schema`
  (`order_new_lists_expires_in`); fixture drift, examples reproduced by the constructors, 13 invalid variants; registry, machine lists and the
  legacy-command examples of older tests switched from `rate` to `admcancel`/`dispute.list`.
- **Deviations from the harness contract**: `order mine` `trade_index` and `trade_pubkey` are nullable (a key older than the newest 200 derived
  indexes, or a row without a key); `order rate` `rating` is a JSON integer (the contract named no type); a second dispute or rating is
  `uncertain` (`reply_timeout`) because the node answers it with silence.
- **Not verified**: a real node; the CI Windows runner (after the push).
- **Gates**: fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 971 passed and 0 failed (no FAILED line, none
  ignored; 938 before this batch), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.

### Review Follow-Up Of Specs 009-015: 2026-11-05

Same uncommitted working tree. Five review findings, each with a failing test first where it changes behavior.

- **`order mine` read the keys before the rows** (`commands/order/mine.rs`): an order saved between the two reads had a key newer than the
  ones derived, so its `trade_index` came back null. **Failing first**: `an_order_saved_between_the_reads_never_lacks_its_index`, a unit test
  through a private `OrderSource` seam whose double saves another order (index reserved, then row) right after each read: on the old order the
  new row "has no index". **The fix**: rows first, then keys; an index is reserved before its row is saved and only grows, so the later keys
  cover every row read. The six process tests still pass.
- **A `cant-do` for another order was taken as this order's refusal** (`commands/order/held.rs`): it is now a refusal only when the reply
  names the order, otherwise `uncertain` / `reply_malformed`, like an unusable acceptance. **Failing first**:
  `a_refusal_that_names_another_order_or_none_is_uncertain` (both commands, another order and no order): the dispute came back `error`.
  The scripted node's `refusal` now echoes the request's order id, as mostrod's `manage_errors` does (`enqueue_cant_do_msg(request_id,
  inner_message.id, ...)`); every other suite still passes with it. The trade steps (`commands/order/step.rs`) still accept any `cant-do`
  matched by request id; they were not part of this finding and are left for a follow-up.
- **`order rate` dropped warnings in human mode** (`cli/held.rs`): it now calls the same `warn` as `dispute open`. No warning applies to a
  rating today, so there is no behavior to test.
- **`--invoice -` and end of input**: the one-line rule is kept and now documented wherever `-` is described: the help of the four options
  (and so the discovery descriptions, regenerated), the command reference, README and the spec 010 documents: the writer must close
  standard input after the line. A terminal as stdin is refused at once with `invalid_arguments` (`std::io::IsTerminal`) instead of waiting.
  **Failing first**: `a_terminal_as_stdin_fails_at_once_without_being_read` (a reader that panics when touched) failed with "stdin was read".
  No process-level terminal test: the harness has no pseudo-terminal.
- **Release notes**: the removed `dispute` and `rate` forms and their replacements are listed in `docs/commands.md` ("Removed legacy
  commands"), since `CHANGELOG.md` is generated by git-cliff at release. The legacy long flag was `--orderid` (Clap `rename_all = "lower"`),
  which older documents wrote `--order-id`; both, with `-o` and `-r`, are covered by the legacy-spelling tests.
- **The invoice reason** (item added during the review): an `invalid_arguments` caused by the invoice value itself now carries
  `"reason": "invalid_invoice"` on `order new`, `order take-sell`, `order add-invoice` and `bond add-invoice` (`Envelope::invalid_input`,
  `outcome::INVALID_INVOICE`, `cli/write.rs::refuse_invoice`; `cli/stdin.rs` tells a refused content from a form that is not allowed);
  every other `invalid_arguments` carries none. The schemas now tie the reason to its code (`OutcomeRules::input_reasons`, an `anyOf` in the
  failure branch: any snake-case word on `node_refused`, `invalid_invoice` on `invalid_arguments` where allowed, `"reason": false`
  otherwise); every schema with a reason was regenerated, the pilot listing schema did not change. **Failing first**:
  `an_invoice_the_client_refuses_carries_the_reason_invalid_invoice` (four commands, literal and stdin, JSON and human; the other argument
  errors keep no reason) and `the_schemas_tie_the_reason_to_its_code` failed, and the bad-stdin test (updated to expect the reason) failed with
  `null`. Examples: `invalid_invoice` (003), `sell_invalid_invoice` (004), `order_invalid_invoice` and `bond_invalid_invoice` (005), written from
  the constructors by a temporary printer test, then the printer was removed. The program does not decode a BOLT-11 or check its network;
  the node's refusal of such an invoice stays `node_refused` with its reason.
- **Gates** (review round and the invoice reason, final tree): fmt, clippy `--all-targets --all-features -D warnings` (a first run flagged
  `type_complexity` in the new `order mine` test double; it now uses a named struct), `cargo test --locked` 976 passed and 0 failed (no FAILED
  line, none ignored; 971 before), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.
- **The refusal rule in the trade steps** (follow-up): `order fiat-sent`, `order release` and `order cancel` now apply the same rule as
  `dispute open` and `order rate`, through the shared `held::refusal` (a `cant-do` is the node's refusal only when it names the order;
  otherwise `uncertain` / `reply_malformed`, and a range continuation's reserved index stays spent, persistence `applied`, since the node may
  hold the key; only a real refusal gives it back). **Failing first**: `a_refusal_that_names_another_order_or_none_is_uncertain_for_every_step`
  (`tests/step_process.rs`: three steps, another order and none, plus a range release) failed with `error` for `fiat-sent`. The spec 007
  spec, data model and contract and the architecture note are amended.
- **Gates** (the trade-step refusal rule): fmt, clippy, `cargo test --locked` 977 passed and 0 failed, build, MSRV 1.94.0.

## Reply Deadline (spec 016): --reply-timeout: 2026-11-05

Same uncommitted working tree. The reason: a live harness run against a regtest mostrod (v0.20.2, SQLite in rollback-journal mode) got a
`take-sell` answer after 24 s, which the fixed 15 s reply budget had already turned into `uncertain`.

- **Failing first**: `tests/reply_timeout_process.rs` 0/4 (the option did not exist: every run was `invalid_arguments`).
- **The change**: `--reply-timeout SECONDS`, a Clap global (any command depth) with `value_parser!(u64).range(5..=300)` and default 15;
  `cli/write.rs::exchange_limits(cli)` sets `ExchangeLimits::reply` and keeps the connect, probe and cleanup budgets; the eight dispatch sites of
  the eleven waiting commands use it. Public reads and legacy commands ignore it. Discovery gains `global_options` (generated in
  `cli/input_schema.rs`: every global option of the root with its property document, `default` and `applies_to` from the registry);
  global options are no longer command input. `docs/schemas/discovery-v1.json` regenerated (the `schema` output schema gains the field).
- **Tests**: against a silent scripted node, all eleven commands (one sandbox each, run at once) stop between 5 and 12 s with
  `--reply-timeout 5` placed after the command words (`uncertain`/`reply_timeout`; `error` for `order info`); a node answering after 16 s gives
  `ok` with `--reply-timeout 20` and `uncertain` with the default; `4`, `301`, `0`, text and empty are `invalid_arguments` naming
  `order.take-sell`, before and after the command words, nothing sent; `order list` accepts and ignores it; the unit test
  `the_global_options_are_listed_with_what_they_apply_to` (exactly the eleven request and write commands, default `"15"`, not command input).
- **Gates** (final tree): fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 982 passed and 0 failed (no FAILED
  line, none ignored; 977 before), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.

## Advertised Proof Of Work (spec 017): mined, not refused: 2026-11-07

Uncommitted working tree on `main` (738bafd). The reason: mostro-mcp runs `mostro-cli` 0.17.0 with `POW` unset, and the public node
advertises `pow` 6, so every `order new` was `pow_required` (`not_attempted`) with no number in the error, while the CLI already held the
value from its node-info probe.

- **Failing first**: the unit tests for lanes, unmet work, `work_to_mine`, `Envelope::with_work`, the schema rule and `error_line` did not
  compile (the API did not exist). The existing process tests that asserted `pow_required` for a node asking for 9 or 12 bits (order info,
  order new, the takes, add-invoice, the steps) are the record of the old refusal; they failed against the new code (9 failures plus the
  fixture drift) until moved above the ceiling. Regtest: the released 0.17.0 binary, `POW` unset, against a node advertising `pow` 6 and
  `pow_first_contact` 8: `pow_required`, `publication: not_attempted`.
- **The change**: `exchange` probes, then mines `max(--pow/POW, required)` for the request's lane (known: `pow`; first contact:
  `max(pow, pow_first_contact)`) up to `AUTO_POW_CEILING` (20); above it, with a lower floor, `pow_required` before publishing. Every
  `pow_required` carries `error.required` and `error.configured`, also on the timeout path; the human line names both and the flag. Twelve
  schema fixtures regenerated (the commands that can emit `pow_required` and discovery); the pilot fixtures did not change.
- **Tests**: the scripted node now records the leading zero bits of each request (`Received.work`). A node asking for 6, 9 or 12 bits gets
  them mined without `--pow` (order new, take-sell, fiat-sent, add-invoice, order info, in JSON); a first contact clears `pow_first_contact`,
  a known key `pow`, a floor of 14 is kept; 21 bits is refused with nothing published and `required` 21, `configured` 0 in JSON and in the
  human line; a known-lane request dropped by a node that advertises first-contact work is `pow_required` with `required` 16 after the
  timeout. Mining cost (release, Ryzen 7 4800H): 16 bits 0.03 s mean; 20 bits 1.6 s mean, 5.2 s worst of five.
- **Regtest probe** (manual observation, 2026-11-08): a second mostrod (image `mortsom/mostro:git-8111f5e`, v0.20.2, `pow` 6,
  `pow_first_contact` 8, no bond) on the local Mortsom stack's relay and LND, removed afterwards. This build, `POW` unset: `order new` `ok`
  (`published`, `accepted`, order `pending`), request mined with 9 bits; `order cancel` on the known lane `ok` (`canceled`), 9 bits. The
  probe node's info, rates and canceled-order events stay on that local relay.
- **Public node check** (manual observation, 2026-11-08): the public node `82fa8cb9…` (v0.20.2) advertises `pow` 6 and
  `pow_first_contact` 6 on `wss://relay.mostro.network`. A throwaway identity (fresh `MCLI_DIR`), `POW` unset, `order info` on an unknown id:
  the released 0.17.0 from crates.io returns `pow_required`, `publication: not_attempted`; this build returns `ok` (`published`,
  `accepted`, the id in `not_returned`) in 1.8 s, and the request event on the relay has 6 leading zero bits (`nonce` tag 6).
- **Gates** (final tree): fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --locked` 997 passed and 0 failed (none
  ignored), build, MSRV `cargo +1.94.0 check --locked --all-targets --all-features`.

## Node Settings File (spec 018) Unit 1: The JSON-Capable Commands Read It: 2026-11-08

Branch `feat/config-file-read` from `origin/main` (8970c81), the first of four stacked units of issue #244. Automated verification on Linux
only (uid 1000, not root), with loopback relays, the scripted node and isolated sandboxes; no real node or relay was contacted and no
manual run was made. Windows is not verified locally: only the CI runner, after the branch is pushed, will run it.

- **Failing first**: `tests/config_file_process.rs`, run against the unchanged code: 3 of its first 5 tests failed (`order list` from the
  file alone and the precedence test exited 1 with `configuration_missing`; the refused-file test exited 0 because the file was ignored);
  the two guards (local commands, missing data directory) passed, as they must. The unit tests of `settings`, `settings::file` and
  `cli::write` did not compile (32 errors, the API did not exist); the two `cli::context` hint tests failed on the new first line; the
  discovery drift test failed until the fixture was regenerated. `order_new_takes_the_node_relays_and_work_from_the_file` was added after
  the implementation; with the file's values disabled in `node_lookup` (a temporary change, reverted and rebuilt), it and the two other
  positive tests failed.
- **Review fixes** (same day): a set variable that is not UTF-8 fell through to the file (`RELAYS=$'\xff'` next to a file dialed the
  file's relay); it now counts as set but empty (`settings::env_text`), `configuration_missing` as without a file. A data directory that
  cannot be resolved (a symlink loop, a 300-character name) gave `configuration_invalid`; the lenient read now treats any lookup error as
  no file. Failing first: that process case (the listing succeeded through the file's relay), the `settings::file` case
  (`Err(Unreadable)`), the `env_text` test (did not compile) and the drift test after the `required_configuration` description change.
- **The change**: `src/settings.rs` (pure, added to the no-I/O source boundary) parses `KEY=VALUE` (BOM, CRLF, comments, `export`,
  quotes, empty means unset, last line wins), checks the four keys and their values, and gives `setting(file, name, env)`;
  `src/settings/file.rs` reads `config` in the data directory leniently or strictly and creates nothing. `cli/write.rs::node_lookup` reads
  the file once per run and answers each setting from a set variable, else the file; a refusal prints one `hint: <path>: <problem>` and is
  `configuration_invalid`. `request_options` moved from `cli/migrated.rs` to `cli/write.rs`; `order list`, `order info`,
  `resolve_reader` and `resolve_base` use the lookup. `ENV_FILE_HINT` starts "This command does not read any config file". Discovery:
  only the `required_configuration` description changed; no output schema changed and `schema_version` stays `"1"`.
- **Tests**: unit: every accepted form, the README `env.sh` block, six refused keys and a line without `=` by line number, eleven refused
  values (an nsec with a sentinel, `https://`, POW 256, -1 and x, `gift-wrap`, NUL, ESC, a bare CR) never shown, the precedence of a set
  empty variable, both readers on an absent, empty and regular-file data directory (nothing created), a data directory with mode 0600
  (no file leniently, `Unreadable` strictly), a data directory that cannot be resolved (a 300-character component, and a symlink loop,
  which is `Unreadable` strictly), `config` as a directory, non-UTF-8 and mode 000, a variable that is not Unicode (set but empty, over
  the file), and `request_options` through the file lookup. Process: `order list` (JSON and human) and `order new --yes` against the
  scripted node from the file alone, the latter mining at least the file's `POW` 8; option > set variable > file counted on three loopback relays, an empty variable and an empty `-m` missing; an
  `ADMIN_NSEC=<sentinel>` file at `order list`, `order info`, `order new`, `message list` and `dispute open` in both modes with `-m` and
  `-r` given: exit 1, `configuration_invalid`, exactly one `hint:` with the path and line 1, the sentinel nowhere, zero relay
  connections, the seeded data directory byte-identical; `schema`, `identity init`, `identity show` and `order mine` ignore that file; a
  missing data directory stays missing; a set `RELAYS` that is not UTF-8 is missing over the file (Unix only).
- **Gates** (final tree): `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  `cargo test --locked` 1012 passed, 0 failed, 0 ignored (997 before), `cargo build --locked`,
  `cargo +1.94.0 check --locked --all-targets --all-features`: all passed. Diff size: `git diff --shortstat origin/main...HEAD` gives 21 files changed, 1399 insertions, 76 deletions (1475 changed lines).
