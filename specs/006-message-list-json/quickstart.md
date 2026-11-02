# Validation Guide: Message List

**Status**: Implemented and verified on Linux with loopback relays and a scripted mailbox (evidence in `tests/TEST_SUMMARY.md`). Probed once on the maintainer's local bonded
regtest node on 2026-11-02 (steps 1, 3 and 4 below, and two keys of step 2); the rest is open.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test message_list_process          # mailbox, store, operation, binary scenarios, failure matrix
cargo test --locked --test cli_functions message_list    # grammar and registry
cargo test --locked --lib messages                       # input, records, payload summary, schema
cargo test --locked --test integration_tests message     # fixture, proposed examples, drift
cargo test --locked                                      # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
run --output json message list --since 0                 # invalid_arguments, exit 2
run --output json message list                           # configuration_missing, exit 1
run getdm                                                # unrecognized subcommand, exit 2
ls -A "$T"                                               # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| The flow, JSON | one line, `ok`, oldest first, `next_step` and invoices as data, read stages with a snapshot, store byte-identical, no secret or relay address | `json_returns_the_whole_flow_...` |
| The flow, human | each invoice alone on its line, the next step in words with its command | `human_shows_each_invoice_alone_...` |
| Filters | `--since` narrows; `--order` reads only that key; an unknown order asks nothing | `the_window_and_the_order_narrow_the_result`, the operation tests |
| What must not appear | other authors, other keys, old events, non-Mostro text; duplicates once | `what_is_not_the_nodes_mail_...` |
| Limits | 200 keys (warning `trade_keys_limited`), 50 per relay and 200 records (`truncated`) | the three limit tests |
| Bad arguments | exit 2, `invalid_arguments`, no relay contacted | `bad_arguments_exit_2_...` |
| Local failures | `identity_*`, `configuration_*`, nothing created, no relay contacted | `failures_before_any_relay_...` |
| Relay failures | `relay_connection_failed` (no snapshot), `read_timeout` (snapshot) | `an_unreachable_relay_...` |
| Streams | unwritable stdout exits 1; unwritable stderr changes nothing | `streams_that_cannot_be_written_...` |
| Settings of a write | `--pow` / `--transport` do not fail a read | `work_and_transport_settings_...` |
| Read-only store | read without complaint | `a_read_only_store_is_read_without_complaint` |
| Discovery and drift | listed with a generated schema; fixture and examples current | `integration_tests` |

## Real-Node Probe (separately authorized)

The scripted mailbox encodes what the node does from its source (v0.20.1). Before `message list` is called verified against a node, with authorization
(a public relay is outside the authorization of any other probe), record the node version and each answer. Use a throwaway identity with a few trades on the
node under test:

1. Right after `order new` or a take on a bonded node: `message list --since 10` shows `pay_bond_invoice` with the invoice; pay it; run again and see the next
   instruction (`add_invoice` or `pay_hold_invoice`) with its amount.
2. Run it with several trades and confirm that every key's messages arrive, that the relay honors one filter with many recipient keys and note the per-relay limit
   it applies (if a relay refuses, the plan is to split the keys into batches).
3. `--order <id>` returns only that order's messages and the same trade index.
4. A window with nothing in it is `ok` with no messages; a node that is down is `relay_connection_failed` or `read_timeout`.
5. Record whether a message of the node can arrive later than the relay's `since` bound (clock skew), which decides whether a cursor is needed.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --no-fail-fast        # then count the FAILED lines, not only the passes
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
