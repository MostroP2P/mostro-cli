# Implementation Plan: Identity Commands

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented (slices 1-4) with Linux evidence on isolated stores; nothing here
talks to a node.

## Summary

Two local commands. `identity init` reuses the legacy first-run creation, extracted from `db.rs` into `db/create.rs` so both paths create the same
store; `identity show` extends the read-only loader of `identity.rs`. Outcomes, schemas, discovery and rendering follow `order sync` (a local write
that publishes nothing) and the pilot (a read).

## Technical Context

Rust 2021, MSRV 1.94, no new dependency. Tests: unit tests in the library, process tests of the real binary in sandboxes (`tests/identity_process.rs`),
schema fixtures and examples in `tests/integration_tests.rs`, registry and grammar in `tests/cli_functions.rs`. Files at or below 400 production
lines (`db.rs` shrinks under its frozen cap); imports at the top; tests before code.

## Design

1. **Creation** (`db/create.rs`): the first-run `SCHEMA`, `create_private_db_file` (moved from `db.rs`, its `OpenOptionsExt` import now at the
   top under `#[cfg(unix)]`), `initialize` (tables, mnemonic, user) and `create_store(dir)` for `identity init`: refuse an existing store or
   companion file, `ensure_private_dir`, `create_new` 0600, open by file name, `initialize`, close; on failure remove what this run created.
   `db::connect` calls `create_private_db_file` and `initialize` with its existing prints.
2. **Reading** (`identity.rs`): `configured_dir`, and `load_identity_from(dir)` returning the key and the last trade index, refusing a WAL without
   its index file and an index outside `0..=LAST_TRADE_INDEX`.
3. **Outcomes** (`outcome/identity.rs`, `outcome/codes.rs`, `outcome/rules.rs`, `outcome.rs`): `IdentityInitData`, `IdentityShowData`, error
   lists, `ErrorCode::IdentityExists`, `Stages::Local` / `OutcomeRules::local` / `ExecutionMeta::local`, an integer schema for the index.
4. **Operations** (`commands/identity.rs`): `identity_init(dir)` and `identity_show(dir)` map the store results to outcomes.
5. **Commands** (`cli/commands.rs`, `cli/registry.rs`, `cli/identity.rs`, `cli/migrated.rs`, `cli/legacy.rs`, `parse_human/identity.rs`): grammar
   `Commands::Identity { IdentityCommands::{Init, Show} }`, registry entries (`JsonSupport::Read`, like `order sync`), dispatch through
   `cli/write.rs::finish`, human renderers.
6. **Discovery** (`cli/schema.rs`): output schemas, `required_configuration` `[]`, fixtures `identity-init-v1.json`, `identity-show-v1.json`,
   regenerated `discovery-v1.json`.

## Risks

- **Drift from the legacy store**: shared code plus a process test that compares a legacy-created store with an `identity init` store.
- **Secret exposure**: `User` derives `Debug` with the mnemonic; the operation never formats it, and the process tests search every stream for
  the mnemonic and the private key read back from the store.
- **Windows**: the 0600/0700 modes are Unix-only (as for the legacy path); the mode assertions are `#[cfg(unix)]`.
