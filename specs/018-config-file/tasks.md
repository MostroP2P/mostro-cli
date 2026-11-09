# Tasks: Node Settings From A Config File

**Design**: [spec](spec.md), [plan](plan.md), [data model](data-model.md), [CLI contract](contracts/cli.md).

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md): tests before code; gates fmt, clippy `-D warnings`, `cargo test --locked`,
build, `cargo +1.94.0 check --locked --all-targets --all-features`. No push, no real node or relay; loopback relays and the scripted node
only. One `tests/TEST_SUMMARY.md` entry per unit.

- [x] **1. Reading on the JSON-capable path** (`feat/config-file-read`). Failing first: `tests/config_file_process.rs` (3 of 5 failed;
  the order new case was added afterwards and fails with the read disabled), the unit tests of `settings`, `settings::file` and
  `cli::write` (did not compile), the two `cli::context` hint tests and the discovery drift test. Then `settings.rs`, `settings/file.rs`,
  `node_lookup`, the regenerated discovery fixture, docs.
- [x] **2. Legacy path and typed core** (`feat/config-file-legacy`). Failing first: the two `config_file_process` legacy tests (exit 1
  `MOSTRO_PUBKEY not set`; exit 0 with `mcli.db` created despite the refused file), the new unit tests of `settings`, `settings::file`,
  `outcome::config`, `commands::config`, `parse_human::config`, `cli::input_schema`, `outcome::tests` and the two `schema_fixtures`
  example tests (did not compile), then the two renamed `cli::context` message tests (failed until the reword). Then `settings::var` and
  `install`, the `init_context` read, `update_config`, `configuration_unwritable`, `x-positional`, `examples.json`, docs. Review fixes:
  see `tests/TEST_SUMMARY.md`; the `config show` core moved to unit 4.
- [x] **3. `config set`** (`feat/config-set`). Failing first: `tests/config_process.rs` (9 of 9 failed: no `config` command), the
  registry and grammar tests of `tests/cli_functions.rs` (did not compile), three `schema_fixtures` tests and the legacy count of
  `order_list_process`. Then the grammar, `cli/config.rs`, the registry entry, the schema arm, the regenerated `discovery-v1.json` and the
  new `config-set-v1.json`, docs. Moved to unit 4 to stay under 1500 changed lines: the set-then-`order list` round-trip test, and the
  guidance rewording (`ENV_FILE_HINT`, the missing-setting messages and their tests, the README setup).
- [x] **4. `config show`** (`feat/config-show`), with its typed core and, from unit 3, the set-then-`order list` round-trip test and the
  guidance rewording. Failing first: `tests/config_process.rs` (7 of 7 new failed: no `config show`), the new unit tests of
  `outcome::config` and `commands::config`, `tests/cli_functions.rs` and `tests/integration_tests.rs` (did not compile), then the two
  renamed `cli::context` message tests (failed until the reword) and four `schema_fixtures` tests (failed until the fixtures were
  regenerated). Then `config_show_rules`, `config_show` and `show_outcome`, the grammar, the dispatch, the registry entry, the schema arm,
  `config-show-v1.json` and the regenerated discovery, the three `config.show` examples, docs.
- [x] **5. Review fixes** (`feat/config-review-fixes`), stacked on unit 4: unit 2's second review round (cherry-picked from its local
  branch) and unit 3's review, which do not fit those units' 1500 lines. Failing first, on this branch:
  `settings::file::tests::update_refuses_a_symlinked_lock` (`Ok` instead of `NotCreated`, the link's target created outside the data
  directory, with `O_NOFOLLOW` removed) and the two `cli::context` message tests (until the hint said that a set variable wins over the
  file). Then `O_NOFOLLOW` on `.config.lock`, the temporary written through its handle, the directory sync, the flag docs, the owner-only
  claims limited to Unix and the `tests/README.md` counts. The directory sync and the docs have no test. From the review of this unit,
  failing first: `cli::context::tests::blank_mostro_pubkey_gets_the_missing_message` (`Ok("")`) and
  `an_empty_variable_hides_the_file_from_a_legacy_command` in `config_file_process` (`Invalid input length 64`); then the blank key
  treated as missing, `libc` without its default features (no test: the build is unchanged) and the 14 missing test targets listed.
