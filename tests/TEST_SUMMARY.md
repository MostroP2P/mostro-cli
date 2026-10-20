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