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
- [ ] **3. `config set`** (`feat/config-set`).
- [ ] **4. `config show`** (`feat/config-show`), with its typed core: `config_show`, `config_show_rules` and its examples.
