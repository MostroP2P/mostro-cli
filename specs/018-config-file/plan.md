# Implementation Plan: Node Settings From A Config File

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-08 | **Status**: Units 1 to 4 implemented with Linux evidence.

## Units

| Unit | Branch | Content |
| --- | --- | --- |
| 1 | `feat/config-file-read` | `settings` (format, checks, precedence) and `settings/file` (both reads); the JSON-capable networked commands read the file |
| 2 | `feat/config-file-legacy` | the human-only commands read it (`settings::var`, installed once by `init_context` before `mcli.db`); the typed core of `config set`: `update_config` (under a lock), `outcome/config.rs`, `commands/config.rs`, `parse_human/config.rs`, `configuration_unwritable` and `x-positional` (both proposed, see Proposals) |
| 3 | `feat/config-set` | `config set`: grammar, dispatch, registry, schema fixture, guidance |
| 4 | `feat/config-show` | `config show`: its typed core (moved from unit 2, see below), grammar, dispatch, registry, schema fixture; the spec becomes implemented |

This split is a delivery proposal of the workflow, not a maintainer decision. Each unit stays at or below 1500 changed lines (`git diff --shortstat <base>...HEAD`, fixtures, specs and tests included). Fallbacks move
whole pieces with their tests: unit 1 moves the `config set` and `config show` contract sections to unit 2; unit 2 moved the `config show`
core (`config_show`, its rules and examples) to unit 4 after its review fixes; unit 3 moved the set-then-list round-trip test and then
the guidance rewording to unit 4, since its two generated fixtures alone take about 770 lines.

## Unit 1 design

- `src/settings.rs` (pure, on the no-I/O source boundary): `Setting` (`name`, `from_name`, `accepts`), `ConfigFile::parse` and `get`,
  `ConfigFileError` with fixed texts, and `setting(file, name, env)`: the environment's value when set, else the file's.
- `src/settings/file.rs`: `config_path`, the private `read_with(dir, is_absent)` behind `read_config` (lenient) and `read_config_strict`
  (used from unit 3), `ConfigProblem` (`<path>: <problem>`) and `configured_config`.
- `cli/write.rs`: `node_lookup` reads the file once and returns the lookup (environment, then file); a refusal prints `hint:` and is
  `configuration_invalid`. `request_options` moved here from `cli/migrated.rs`; `resolve_reader` and `resolve_base` share
  `reader_with`, so the order of checks is file, configuration, request settings, identity, store.
- `cli/migrated.rs`: `order list` (inside the configuration closure, after filter validation) and `order info` use `node_lookup`.
- `cli/context.rs`: `ENV_FILE_HINT` now starts "This command does not read any config file", which stays true until unit 2.
- `cli/schema.rs`: the `required_configuration` description names the option, the variable and the file key; discovery regenerated.

## Decisions

- **Why `configuration_invalid` for a bad file**: the settings are unusable, as with a bad option; the code, its message and exit are
  unchanged and only gain a source, so they stay in v1. A refusal of the whole file, even when options supply every value, keeps a
  secret written into the file from going unnoticed.
- **Why two reads**: a lenient read keeps sandboxes that hide the home directory working; a strict one keeps `config set` from rewriting a
  file it could not read and `config show` from reporting nulls it did not see.
- **Not done (minimal scope)**: source attribution, an override warning, size limits, duplicate-key refusal, value normalization, a
  `config unset`, and an owner or group- and world-writable refusal: whoever can write the data directory can already replace `mcli.db`,
  which holds the identity, so the directory's mode (the README creates it with 0700) is the protection, as it is for `mcli.db`.

## Proposals awaiting maintainer approval

- **One new code, `configuration_unwritable`** (unit 3, `config.set` only). Strictly needed because no existing code says that the file
  could not be written: `store_unwritable` names `mcli.db`, which `config set` never touches; `configuration_invalid` would say the
  settings are wrong; `internal_error` is for defects. It is additive: no existing schema lists it.
- **One discovery keyword, `x-positional: N`** (unit 2, additive within v1): `config set <key> <value>` is the first machine command with
  positional arguments, and without it an input schema cannot say that `key` and `value` are not options.

## Risks

- Between units 1 and 2 the JSON-capable commands read the file and the human-only ones do not; documented, and the stack is meant to be
  merged in order without a release in between.
- An eager read: a broken file fails every networked command, even `message list` and `order sync`, which do not use `POW` or
  `TRANSPORT`; the hint names the file and the line.
- A set but empty variable wins over the file (as an empty option does); documented.
