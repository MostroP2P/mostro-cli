# Validation Guide: Add Invoice

**Status**: Implemented and verified on Linux with loopback relays and a scripted fake daemon (evidence in `tests/TEST_SUMMARY.md`; CI, Windows included, was
green through slice 4b). `order add-invoice` was probed on the maintainer's local regtest node on 2026-11-01 (steps 1, 2 and 3 below; see `tests/TEST_SUMMARY.md`). `bond add-invoice` was not
probed: it needs a slashed bond.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test invoice_process             # fixtures, store, operations, binary scenarios, failure matrix
cargo test --locked --test cli_functions invoice_grammar
cargo test --locked --lib invoice                      # input, outcomes, schemas, order-id correlation
cargo test --locked --test integration_tests invoice   # fixtures, proposed examples, drift
cargo test --locked                                    # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
O=61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0
run --output json order add-invoice --order $O --invoice lnbc1x          # invalid_arguments, exit 2 (no --yes)
run --output json bond add-invoice --order $O --invoice lnbc1x --yes     # configuration_missing, exit 1
run --output json bond add-invoice --order $O --invoice a@b.c --yes      # invalid_arguments, exit 2 (an address)
run addinvoice -o $O -i lnbc1x                                           # unrecognized subcommand, exit 2
ls -A "$T"                                                               # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Order invoice accepted, JSON `--yes` | `ok`, `waiting_seller_payment`, one request without a trade index, local status `waiting-payment`, index unchanged, document conforms to `order-add-invoice-v1.json`, no secret or invoice in any stream | `order_add_invoice_in_json_is_accepted_...` |
| Seller already paid | `ok`, `payment_accepted` with the order, status `active` | `a_seller_who_already_paid_...` |
| Replacement invoice | `ok`, `invoice_updated`, persistence `not_attempted`, store byte-identical | `a_replacement_invoice_changes_nothing_locally` |
| Bond invoice accepted | `ok`, `invoice_accepted` with the share, matched although the ack has no request id, nothing written | `bond_add_invoice_in_json_is_accepted_...`, `an_acknowledged_bond_invoice_...` |
| Older node's silence on a bond | `uncertain` / `reply_timeout` after 15 s, one request, never read as success | `an_older_nodes_silence_is_uncertain_...`, `a_silent_node_is_uncertain_...` |
| JSON without `--yes` | `invalid_arguments` exit 2, nothing sent, store untouched, both commands | `json_without_yes_does_nothing_...` |
| Prompt | blank, `y`, `Y`+CRLF accept; `n`, other text, empty input and closed stdin decline with nothing sent; the preview never shows the invoice | `the_prompt_accepts_y_and_blank_...` |
| Unknown order or no key | `order_unknown`, nothing sent, no question asked | `an_unknown_order_fails_before_any_question_...` |
| Node refuses | `node_refused` with reason, `published/rejected/not_attempted`, nothing written | `a_refusal_shows_its_reason_...`, `every_refusal_...` |
| Unusable reply, decoys | `uncertain` / `reply_malformed`; stale request ids, other authors, other addressees and acks for other orders ignored | `replies_that_do_not_fit_...`, `decoys_are_ignored_...`, `bond_decoys_...` |
| Local status update fails | `ok` with persistence `failed` and warning `local_status_stale`, exit 0 | `a_failed_status_update_...`, `a_stale_local_status_...` |
| Local failures (identity, configuration, protocol, work, rejected and unreachable relay) on both commands | `error`, `not_attempted`, no request, store byte-identical, no sentinel, relay address or invoice | `failures_before_any_request_...` and the tests beside it |
| Store | the order invoice needs a writable store; the bond invoice does not | `only_the_order_invoice_needs_a_writable_store` |
| Invalid input | exit 2, resolved command (`null` for the removed spellings), nothing echoed | `invalid_input_is_an_argument_error_...` |
| Unwritable streams | exit 1 with one fixed line; the document is unaffected by a closed stderr | the two stream tests |
| Discovery and drift | both commands listed with generated schemas; fixtures and examples current; boundaries and CI wiring green | `integration_tests` |

## Regtest Probe (Linux, separately authorized)

Process fixtures encode the daemon's behavior from its source (v0.20.1; regtest pins v0.19.3). Before `order add-invoice` is called verified against a
daemon, run a controlled probe on the MCP repository's regtest network with regtest identities only (never a public node, never `devenv.sh clean` as
setup), and record the daemon version and each answer:

1. As a maker `order new --side sell`; as a second identity `order take-sell --order <id>` with no invoice, then `order add-invoice --order <id> --invoice <regtest invoice>`
   (`devenv.sh invoice` makes one): record the reply (`waiting-seller-to-pay` with no payload, and whether the request id is echoed), the local status and that the index did not move.
2. A second `order add-invoice` for the same order, and one from the maker's identity: record the refusals (`not_allowed_by_status`, `invalid_peer`).
3. An invoice the node refuses (wrong amount): record `invalid_invoice`.
4. After the seller pays the hold invoice (`devenv.sh pay`), the order is active: record whether a late invoice is refused.
5. Stop the node during the request to see `uncertain`, then confirm the state with `order info --order <id>`.
6. `bond add-invoice` has no regtest case (no bonds, no slashing); it stays verified against the scripted node only, and the node-side `bond-invoice-accepted` with
   no request id is an assumption from the v0.20.1 source until a bonded node is available.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --no-fail-fast        # then count the FAILED lines, not only the passes
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
