# Validation Guide: Order New

**Status**: Implemented and verified on Linux and the CI-hosted Windows runner with loopback relays and a scripted fake daemon
(evidence in `tests/TEST_SUMMARY.md`). The regtest probe below has not been run; no claim about a real daemon is made.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test order_new_process          # operation, binary scenarios, failure matrix
cargo test --locked --test cli_functions order_new_grammar
cargo test --locked --test integration_tests new      # schema fixture, proposed examples, drift
cargo test --locked                                   # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
A="order new --side sell --currency USD --fiat-amount 100 --payment-method SEPA"
run --output json $A          # invalid_arguments, exit 2 (no --yes)
run --output json $A --yes    # configuration_missing, exit 1
run neworder -k sell          # unrecognized subcommand, exit 2
ls -A "$T"                    # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Accepted order, JSON `--yes` | `ok`, one request, row and index saved together, document conforms to `order-new-v1.json`, no secret in any stream | `json_with_yes_creates_one_order_...` |
| JSON without `--yes` | `invalid_arguments` exit 2, nothing sent, store untouched | `json_without_yes_does_nothing_at_all` |
| Prompt | blank, `y`, `Y`+CRLF accept; `n`, other text, empty input and closed stdin decline with nothing sent | `the_prompt_accepts_y_and_blank_...` |
| Node refuses | `node_refused` with reason, `published/rejected/not_attempted`, nothing written | `a_refusal_shows_its_reason_...` |
| No reply | `uncertain` / `reply_timeout` after 15 s, one request, index spent, no order row | `a_silent_node_is_uncertain_...` |
| Unusable reply | `uncertain` / `reply_malformed`, index spent | `silence_and_an_odd_reply_...` |
| Bonded node | `error` / `bond_required`, index spent | `a_bonded_node_is_an_error_...` |
| Accepted but not saved | `uncertain` / `order_not_saved` with the node's order, index still spent | `an_order_that_cannot_be_saved_...`, `a_failed_order_save_still_spends_...` |
| Range order | `min`/`max` on the wire with fiat amount 0, both bounds saved | `a_range_order_reaches_the_node_...` |
| Local failures (identity, configuration, protocol, work, rejected relay, unreachable relay, unwritable store) | `error`, `not_attempted`, no request, store byte-identical, no sentinel | `failures_before_any_request_...` and the tests beside it |
| Invalid input | exit 2, resolved command `order.new` (`null` for `neworder`), nothing echoed | `invalid_input_is_an_argument_error_...` |
| Unwritable streams | exit 1 with one fixed line; the document is unaffected by a closed stderr | the two stream tests |
| Discovery and drift | `order.new` listed with generated schemas; fixtures current; boundaries and CI wiring green | `integration_tests` |

## Regtest Probe (Linux, separately authorized)

Process fixtures encode the daemon's behavior from its source (v0.20.1; regtest pins v0.19.3). Before the feature is called verified
against a daemon, run a controlled probe on the MCP repository's regtest network with regtest identities only (never a public node,
never `devenv.sh clean` as setup), with an existing local identity, and record the daemon version and each answer:

1. `order new` fixed order, then `order list` and `order info --order <id>` for it; the store holds the row and index 1.
2. A second order (index 2); then a deliberately stale index (restore an older `last_trade_index`) to see `invalid_trade_index`.
3. A range order, a market-priced order, an invalid currency and a half-specified range (`invalid_amount`) to record the refusals.
4. A bonded node, if one is available: confirm `pay-bond-invoice` and the `bond_required` outcome.
5. Stop the node during a run to see `uncertain`, then confirm with `order list` whether an order exists.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
