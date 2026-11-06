# CLI Contract: Identity Commands

Builds on the [order-list contract](../../001-order-list-json/contracts/cli.md) (streams, exit codes, one document, diagnostics on stderr) and
the local-write stages of [order sync](../../008-range-children/contracts/cli.md).

## Grammar

```text
mostro-cli identity init [--output human|json]
mostro-cli identity show [--output human|json]
```

No options of their own. Canonical JSON names: `identity.init`, `identity.show`. Both need no node, relay, Mostro public key, stdin or `--yes`;
they read `MCLI_DIR` (else the home directory's `.mcli`) and nothing else. Root options are accepted and ignored.

## identity init

Creates the data directory (owner-only, one level, an existing one tightened to owner-only) and `mcli.db` exactly as the legacy first run does:
the same tables, owner-only file, one user row with a fresh 12-word mnemonic and no trade index. It publishes nothing and asks nothing. If
`mcli.db`, `mcli.db-journal`, `mcli.db-wal` or `mcli.db-shm` already exists it changes nothing and fails with `identity_exists`.

## identity show

Reads the store read-only (no file is created or changed, even beside a WAL store) and reports the identity public key and the last trade index.

## Secrets

The mnemonic and every private key stay in `mcli.db`; no mode, stream or error shows them. There is no `--reveal`.

## Human output

`identity init`: a title, the identity public key and a line saying the mnemonic stays in the local store. `identity show`: the identity public
key and the last trade index. Errors are one fixed line on stderr. `--verbose` prints one line `verbose: <command> <code or ok>`.

## Stable errors

`identity.init`: `invalid_arguments` (exit 2), `identity_exists` (new), `store_unwritable`, `internal_error`. `identity.show`: `invalid_arguments`
(exit 2), `identity_missing`, `identity_unreadable`, `internal_error`. Statuses are `ok` and `error` only. Stages are in the
[data model](../data-model.md).
