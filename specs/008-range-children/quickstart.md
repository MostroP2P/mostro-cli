# Validation Guide: Range Order Children

**Status**: Implemented and verified on Linux with loopback relays, a scripted node and a scripted mailbox (evidence in `tests/TEST_SUMMARY.md`; CI is
checked after the push). Not probed on a real node: the probe procedure below is open.

## Focused Commands

From the CLI root:

```sh
cargo test --locked --test sync_process                # order sync: operation, binary, failure matrix
cargo test --locked --test step_process                # keys and indexes in the steps, children of the scripted node
cargo test --locked --test cli_functions order_sync    # grammar and registry
cargo test --locked --lib sync                         # input, data, schema rules
cargo test --locked --test integration_tests sync      # fixture, 10 examples, invalid variants, drift
cargo test --locked                                    # everything
```

## Offline Smoke Check

With an empty environment and a sandboxed home, nothing is created:

```sh
T=$(mktemp -d)
run() { env -i PATH="$PATH" HOME="$T" USERPROFILE="$T" MCLI_DIR="$T/m" ./target/debug/mostro-cli "$@" </dev/null; echo " -> exit $?"; }
run --output json order sync                    # configuration_missing, exit 1
run --output json order sync --since 0          # invalid_arguments, exit 2
run --output json order sync --yes              # invalid_arguments, exit 2 (no such option)
ls -A "$T"                                      # empty
```

## Acceptance Matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Maker marker | `order new` saves the user's own order, `order take-*` does not; only the maker's range has room | `the_maker_marker_is_exact_...`, the take and new process tests |
| Key and index in the steps | a seller's `release` and a buyer's `fiat-sent` on a range order carry a key at the next index; nothing for a taker, a fixed order, an exhausted range, the wrong step or a cancel | `the_makers_own_step_...`, `no_key_for_...` |
| Index policy | kept on a refusal and when nothing left; spent on silence, an unusable reply and an accepted step; an unrecordable index is `trade_index_not_recorded` | `the_index_is_spent_...`, `a_trade_index_that_cannot_be_recorded_...` |
| The node creates children | a scripted range order creates its child for the key it was given, equal remainder fixed, below minimum none | `mod children` |
| `order sync` adds an unknown announced order | a row with the key of its index, the user's own marker, usable at once | `an_announced_order_...`, `an_adopted_child_can_be_acted_on_at_once` |
| Idempotent, never replaces | a second run saves nothing and changes no byte; a known order keeps its row | `a_second_run_...`, `a_known_order_is_never_replaced` |
| What counts | other actions, authors, recipients, ids, windows are ignored; the newest announcement wins | `only_announcements_...`, `the_newest_announcement_...` |
| A save fails | `uncertain` / `order_not_saved` with the rows already saved | `a_save_that_fails_...` |
| Failures before a row | stable codes in every mode, nothing written or created, no secret | `a_missing_damaged_or_empty_identity_...`, the binary tests |
| Settings of a request | `--pow`/`--transport` do not concern a sync | `work_and_transport_settings_...` |
| Discovery and drift | `order.sync` listed with a generated schema; fixture and examples current | `integration_tests` |

## Regtest Probe (Linux, separately authorized)

Process fixtures encode the daemon's behavior from its source (v0.20.1; regtest pins v0.19.3). Before range continuation is called verified, run a
controlled probe on the maintainer's regtest node with throwaway identities, and record the daemon version and each answer:

1. A **sell** range order (`order new --side sell --fiat-amount 50 --fiat-amount-max 150`); a taker takes 50 (`--amount 50`) and the trade runs to `fiat-sent`
   (bond, buyer invoice and hold invoice are paid by the maintainer). The maker runs `order release`: record `next_trade_index`, `next_step`
   `sync_child_order` and the local index.
2. The maker runs `order sync`: record the child order added (expected: a range of 50 to 100, `pending`, the index of the key sent), then `order info` and
   `order cancel` on it; run `order sync` again and expect nothing to add.
3. A **buy** range order: the maker's `fiat-sent` carries the key and the node keeps it; after the seller's `release`, `order sync` finds the child.
4. A taker's `fiat-sent` and `release` on a range order they took: record that no key is sent and no index is spent.
5. A slice that leaves less than the minimum (take 120 of 50-150): record that the key is ignored (one index spent) and `order sync` finds nothing.

## Required Gates And Evidence

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --no-fail-fast        # then count the FAILED lines, not only the passes
cargo build --locked
cargo +1.94.0 check --locked --all-targets --all-features
```

Record command, platform, toolchain, result and blockers in `tests/TEST_SUMMARY.md`.
