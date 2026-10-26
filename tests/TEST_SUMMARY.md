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