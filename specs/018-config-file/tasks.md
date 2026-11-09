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
