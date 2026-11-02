# Validation Guide: Trade Steps

**Status**: Implemented and verified on Linux with loopback relays and a scripted node (evidence in `tests/TEST_SUMMARY.md`); CI is checked after the
push. Probed on the maintainer's local regtest node on 2026-11-02 (steps 1, 2 and 3 below, and a cooperative cancel; see `tests/TEST_SUMMARY.md`); steps 4 and 5 were not.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test step_process                # fixture, store, operations, binary scenarios, failure matrix
cargo test --locked --test cli_functions trade_step    # grammar and registry
cargo test --locked --lib step                         # outcomes, schemas, human output, range rule
cargo test --locked --test integration_tests step      # fixtures, 16 examples, invalid variants, drift
cargo test --locked                                    # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
O=61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0
run --output json order release --order $O                # invalid_arguments, exit 2 (no --yes)
run --output json order cancel --order $O --yes           # configuration_missing, exit 1
run --output json order fiat-sent --order not-a-uuid --yes # invalid_arguments, exit 2
run fiatsent -o $O                                        # unrecognized subcommand, exit 2
ls -A "$T"                                                # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Each step accepted, JSON `--yes` | `ok`, the result and next step, one request with no payload, no index and no next trade key, local status per the data model, index unchanged, document conforms to its fixture, no secret in any stream | `each_step_in_json_is_accepted_...`, the operation tests |
| Cooperative cancel, both steps | `cancel_requested` (nothing recorded locally), then `cancel_accepted` (`cooperatively-canceled`) | `the_first_step_of_a_cooperative_cancel_...`, `confirming_the_peers_cooperative_cancel_...`, `the_two_cooperative_cancel_steps_...` |
| JSON without `--yes` | `invalid_arguments` exit 2, nothing sent, store untouched, all three | `json_without_yes_does_nothing_...` |
| Prompt | blank, `y`, `Y`+CRLF accept; `n`, other text, empty input and closed stdin decline with nothing sent; the release preview says it cannot be undone | `the_prompt_accepts_y_and_blank_...`, `the_release_preview_warns_...` |
| Unknown order or no key | `order_unknown`, nothing sent, no question asked | `an_unknown_order_...`, `an_order_with_no_local_row_...` |
| Node refuses | `node_refused` with reason, `published/rejected/not_attempted`, nothing written | `every_refusal_...`, `a_refusal_shows_its_reason_...` |
| Silence | `uncertain` / `reply_timeout` after the production budget, one request, advice per step | `silence_is_uncertain_...`, `a_silent_node_is_uncertain_...` |
| Unusable reply, decoys | `reply_malformed` or a timeout; stale ids, other authors, other addressees and replies without the request id ignored | `replies_that_do_not_fit_...`, `decoys_are_ignored_...`, `a_reply_without_the_request_id_...` |
| Local status update fails | `ok`, persistence `failed`, warning `local_status_stale`, exit 0 | `a_failed_status_update_...`, `a_stale_local_status_...` |
| Range order the user made, with room left | the maker's step carries a key at the next index, `next_trade_index` and `sync_child_order`; nothing for a taker, a fixed order, an exhausted range or a cancel; the index is kept on a refusal and spent on silence | spec 008: `the_makers_own_step_...`, `no_key_for_...`, `the_index_is_spent_...` |
| Local failures on all three steps | `error`, `not_attempted`, no request, store byte-identical, no sentinel or relay address | `failures_before_any_request_...` and the tests beside it |
| Invalid input, unwritable streams | exit 2 with nothing echoed; exit 1 with one fixed line | `invalid_input_is_an_argument_error_...`, the two stream tests |
| Discovery and drift | three commands listed with generated schemas; fixtures and examples current | `integration_tests` |

## Regtest Probe (Linux, separately authorized)

Process fixtures encode the daemon's behavior from its source (v0.20.1; regtest pins v0.19.3). Before the steps are called verified against a daemon,
run a controlled probe on the maintainer's regtest node with regtest identities only (never a public node unless authorized), and record the daemon
version and each answer:

1. A full trade with a paying seller: `order new --side sell` as the maker, `order take-sell` as the taker (pay a bond if the node asks), `order add-invoice`, the
   seller pays the hold invoice, then `order fiat-sent` as the buyer and `order release` as the seller. Record each reply (`fiat-sent-ok`, whether the seller
   gets `hold-invoice-payment-settled` or `released`, and whether the request id is echoed), the local statuses, and that the index did not move.
2. Refusals: `fiat-sent` as the seller, `release` as the buyer, `release` before fiat was sent, a second `fiat-sent`.
3. `order cancel`: a pending order by its maker; a waiting order by the taker; an active trade as a cooperative cancel from both sides.
4. A range order: `fiat-sent` and `release` with room left, to see the key, the child order and what the node does without the next trade key.
5. Stop the node during a request to see `uncertain`, then confirm the state with `order info --order <id>`.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --no-fail-fast        # then count the FAILED lines, not only the passes
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
