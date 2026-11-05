# Validation Guide: Order Take

**Status**: Implemented and verified on Linux with loopback relays and a scripted fake daemon (evidence in `tests/TEST_SUMMARY.md`; CI is green to
slice 5, Windows included). The regtest probe below has not been run; no claim about a real daemon is made.
The regtest probe below ran on 2026-11-01 against mostrod v0.19.3 for the main paths and refusals (see `tests/TEST_SUMMARY.md`); bonded and Cashu nodes are not covered.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test order_take_process          # operation, binary scenarios, failure matrix
cargo test --locked --test cli_functions order_take_grammar
cargo test --locked --test integration_tests take      # schemas, fixtures, proposed examples, drift
cargo test --locked                                    # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
O=61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0
run --output json order take-sell --order $O         # invalid_arguments, exit 2 (no --yes)
run --output json order take-buy --order $O --yes    # configuration_missing, exit 1
run takesell -o $O                                   # unrecognized subcommand, exit 2
ls -A "$T"                                           # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Take-sell without an invoice, JSON `--yes` | `ok`, `next_step` `add_invoice`, one request, the node's order and the index saved together, document conforms to `order-take-sell-v1.json`, no secret in any stream | `a_take_sell_in_json_asks_for_an_invoice_...` |
| Take-sell with an invoice | `ok`, `wait_for_seller_payment`, null order, minimal row saved; the invoice appears in no stream and is not stored | `the_invoice_the_user_gives_never_comes_back_...` |
| Take-buy | `ok`, `pay_hold_invoice`, the hold invoice in `data.payment_request`, alone on its line in human mode and never on stderr | `a_take_buy_returns_the_hold_invoice_...` |
| Range take | the amount reaches the node, the document and the row | `the_amount_of_a_range_take_...` |
| JSON without `--yes` | `invalid_arguments` exit 2, nothing sent, store untouched, both sides | `json_without_yes_does_nothing_...` |
| Prompt | blank, `y`, `Y`+CRLF accept; `n`, other text, empty input and closed stdin decline with nothing sent | `the_prompt_accepts_y_and_blank_...` |
| Node refuses | `node_refused` with reason, `published/rejected/not_attempted`, no row, the index given back (amended 2026-11-05 by spec 008) | `refusals_carry_their_reason_...` |
| Node refuses a stale index (amended 2026-11-05 by spec 008, review of slice 7) | `node_refused` / `invalid_trade_index`, `published/rejected/applied`, no row, the index kept: the next run carries the one after it | `a_stale_index_refusal_...`, `refusals_carry_their_reason_...` |
| No reply | `uncertain` / `reply_timeout` after 15 s, one request, index spent, no row | `a_silent_node_is_uncertain_...` |
| Unusable reply (another order, wrong action for the side) | `uncertain` / `reply_malformed`, index spent | `replies_that_do_not_fit_the_take_...` |
| Bonded node | `ok` / `pay_bond_invoice`, bond invoice in `data.payment_request`, row and index saved | `a_bonded_node_returns_the_bond_invoice_...` |
| Accepted but not saved | `uncertain` / `order_not_saved` with the data (hold invoice included), index spent before publishing | `a_take_that_cannot_be_saved_...`, `an_accepted_take_that_cannot_be_saved_...` |
| Local failures before the reservation (identity, configuration, unwritable store) on both takes | `error`, `not_attempted`, no request, store byte-identical, no sentinel, relay address or hold invoice | `failures_before_any_request_...`, `a_store_that_cannot_be_written_...` |
| Failures after the reservation that send nothing (protocol, work, rejected and unreachable relay) on both takes (amended 2026-11-05 by spec 008) | `error`, `not_attempted`, no request, orders and raw trade index unchanged (index given back), no sentinel, relay address or hold invoice | `a_node_that_cannot_be_served_...`, `a_relay_that_rejects_...`, `an_unreachable_relay_...` |
| Invalid input | exit 2, resolved command (`null` for `takesell`/`takebuy`), nothing echoed | `invalid_input_is_an_argument_error_...` |
| Unwritable streams | exit 1 with one fixed line; the document is unaffected by a closed stderr | the two stream tests |
| Discovery and drift | both commands listed with generated schemas; fixtures current; boundaries and CI wiring green | `integration_tests` |

## Regtest Probe (Linux, separately authorized)

Process fixtures encode the daemon's behavior from its source (v0.20.1; regtest pins v0.19.3). Before the feature is called verified against a
daemon, run a controlled probe on the MCP repository's regtest network with regtest identities only (never a public node, never `devenv.sh clean` as
setup), with an existing local identity for the taker, and record the daemon version and each answer:

1. As a maker, `order new --side sell` a fixed order; as a second identity `order take-sell --order <id>` with no invoice: record the reply (`add-invoice`), the row and the index.
2. Another sell order taken with an invoice: record whether the taker hears `waiting-seller-to-pay` with no order, and what the maker receives.
3. A buy order taken with `order take-buy`: record `pay-invoice` and the hold invoice; check the local row.
4. A range order taken with `--amount` inside and outside its bounds; a take of one's own order, of an already taken order and of an unknown id; a second take while one is pending.
5. A stale index (restore an older `last_trade_index`), a bonded node if available, and a node with Cashu escrow enabled: record what is sent.
6. Stop the node during a take to see `uncertain`, then confirm with `order info --order <id>` whether the take happened.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
