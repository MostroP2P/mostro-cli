# Feature Specification: Identity Commands For Humans And Agents

**Feature**: `009-identity-json`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux with isolated stores only (slices 1-4, 2026-11-05; see `tests/TEST_SUMMARY.md`). These commands
talk to no node and no relay, so there is no node or regtest claim to make. CI is checked after the push. Progress is tracked only in
[tasks.md](tasks.md)

**Input**: An end-to-end harness (Mortsom) drives `mostro-cli` as a subprocess through `--output json`. Every migrated command that signs
(`order new`, `order take-*`, `order add-invoice`, the trade steps, `order info`, `message list`, `order sync`) needs an existing identity and never
creates one; today only the legacy first run (`init_context` -> `db::connect`) creates it, which needs `MOSTRO_PUBKEY`, `RELAYS`, prints to
stdout and connects to relays. This spec adds `identity init` and `identity show`, two local commands with human and JSON output. Builds on the
[order-list pilot](../001-order-list-json/spec.md) (envelope, discovery, registry) and [order sync](../008-range-children/spec.md) (the stage
vocabulary of a command that writes locally and publishes nothing); none of their documents change.

## Clarifications

### Session 2026-11-05 (the harness contract, with the defaults below open to veto)

- Q: What does `identity init` create? -> A: Exactly what the legacy first run creates: the data directory (`MCLI_DIR`, else `~/.mcli`) owner-only
  (0700 on Unix, created one level deep, an existing one tightened), `mcli.db` created owner-only (0600) with `create_new`, the same two tables and
  one user row with a fresh 12-word mnemonic and no trade index. The legacy first run and the new command share one implementation
  (`store/create.rs`), so the two cannot drift.
- Q: What does it never do? -> A: It contacts no relay or node, reads no Mostro configuration, never shows the mnemonic or any private key (in
  either mode), needs no `--yes` (it creates only local state and spends nothing) and asks nothing.
- Q: What if a store is already there? -> A: `error` / `identity_exists` (new code, exit 1), nothing modified. "There" means `mcli.db` or one of
  its SQLite companion files (`mcli.db-journal`, `mcli.db-wal`, `mcli.db-shm`): a leftover journal would be applied to a new file. Default, open
  to veto: the command never deletes or repairs anything; the user moves the old files away first.
- Q: Stages? -> A: Those of a local write that publishes nothing, like `order sync`: publication and node acceptance `not_applicable`;
  persistence `applied` on success, `not_attempted` when nothing was written, `failed` when the file was created but could not be completed (it is
  then removed again). An invocation rejected before it ran reports `not_applicable` three times, as for `order sync`.
- Q: What does `identity show` report? -> A: The identity public key and the last trade index recorded in the store (`0` when none is recorded),
  read-only through `identity.rs`: no file, directory or sidecar is created or changed. A store whose trade index cannot be read reliably (a
  `-wal` file without its `-shm` file, a value outside `0..=2^31-1`) is `identity_unreadable`.
- Q: Number or decimal string for `last_trade_index`? -> A: A JSON integer, as the harness contract asks. Other outcomes carry trade indexes as
  decimal strings; this one is bounded to `0..=2147483647` (the last normal BIP32 address), so every JSON consumer reads it exactly. Recorded as a
  deliberate difference.
- Q: Revealing the mnemonic? -> A: Out of scope. A future `--reveal` is a secret export (constitution VI) and needs its own spec.

## Scope

`identity init` and `identity show`, their JSON outcomes, discovery entries, schemas, fixtures and examples. Out of scope: restoring from a
mnemonic, revealing secrets, changing the trade index, any network access, MCP.

## Requirements

- **FR-001**: `mostro-cli identity init [--output human|json]` and `mostro-cli identity show [--output human|json]`, canonical names
  `identity.init` and `identity.show`, registered and listed by `schema`. They take no options; anything else is `invalid_arguments` (exit 2)
  before any effect. Root options (`--mostro-pubkey`, `--relays`, ...) are ignored, as for `schema`.
- **FR-002**: `identity init` creates the store as clarified above, through the code the legacy first run uses, and reports
  `data.identity_pubkey` (64 lowercase hex). Needs only `MCLI_DIR` (or a home directory); no Mostro public key, relays, stdin or `--yes`.
- **FR-003**: An existing store (or companion file) is `identity_exists` with the directory byte-identical. A directory or file that cannot be
  created is `store_unwritable` with persistence `not_attempted`; a file created but not completed is removed again and reported as
  `store_unwritable` with persistence `failed`. Two runs at once create one store: the loser reports `identity_exists`.
- **FR-004**: `identity show` reports `data.identity_pubkey` and `data.last_trade_index` (integer, `0` when none is recorded). No store, or no
  user row, is `identity_missing`; a damaged store is `identity_unreadable`. Nothing is created or changed, whatever the result.
- **FR-005**: Neither command reveals the mnemonic, a private key, the store path or a database error, in any mode or stream. Human output and JSON
  describe the same data.
- **FR-006**: One versioned JSON document per run, generated output schemas (`docs/schemas/identity-init-v1.json`,
  `docs/schemas/identity-show-v1.json`), regenerated discovery, drift tests, `contracts/examples.json` reproduced by the owned constructors.
- **FR-007**: The legacy first run keeps its behavior (it still creates the store when none exists and prints what it printed); its creation
  code moves to `store/create.rs` without a change.

## Success Criteria

- **SC-001**: In an empty sandbox with only `MCLI_DIR` set, `identity init --output json` exits 0 with one valid document; the store it creates
  has the same tables, file mode and directory mode as the one the legacy first run creates, and `identity show` reports the same key.
- **SC-002**: A second `identity init` exits 1 with `identity_exists` and leaves every file byte-identical.
- **SC-003**: After `identity init`, `order new --yes` against the scripted node succeeds with that identity (the store is usable by the migrated
  commands).
- **SC-004**: `identity show` on a missing, empty or damaged store reports `identity_missing` or `identity_unreadable` and changes nothing.
- **SC-005**: No stream of any run contains the mnemonic or a private key; no network connection is made (the commands run with no relay
  configured).
- **SC-006**: Linux gates pass; the CI Windows runner is checked after the push (the mode bits are Unix-only).

## Assumptions

- The legacy first-run code is the reference for "the same store"; it is shared, not copied.
- A store left half-created by a crash is reported as existing (`identity_exists`) and `identity show` reports it as missing or unreadable; the
  user removes it by hand. The command never deletes a file it did not create in the same run.
