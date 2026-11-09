# CLI Contract: Node Settings From A Config File

## Reading (implemented)

No new command or option. Each node setting comes from its root option (even empty), else its environment variable when set (even
empty), else `config` in the data directory. The file format, keys and refusals are in the [data model](../data-model.md). A refused file
is `configuration_invalid` with one stderr `hint: <path>: <problem>` (a human-only command prints `Invalid config file <path>: <problem>`
and exits 1 before `mcli.db` exists); the local commands (`schema`, `identity init`, `identity show`,
`order mine`) and the RPC commands never read it. Discovery's `required_configuration` names the settings each command requires, by
their variable names, which are also the file's keys.

## `config set` (implemented, unit 3)

```text
mostro-cli config set <mostro-pubkey|relays|pow|transport> <VALUE> [--output human|json]
```

Canonical name `config.set`; needs no node, relay, identity, stdin or `--yes`, and connects to nothing. `key` and `value` are
positional (`x-positional` 0 and 1 in discovery); another key is an argument error (`invalid_arguments`, exit 2), and `--` goes before a
value that starts with `-`. The value is trimmed and checked
like the file's (a refusal is `invalid_arguments`, exit 2, never echoed). The existing file is read strictly; a refused or unreachable
one is left untouched (`configuration_invalid` plus the hint). Otherwise the data directory is created one level deep or made owner-only
(0700), and while an exclusive lock on `.config.lock` there (owner-only, never removed, a symlink refused on Unix) is held, so that
`config set` calls made at once all land, the file is read again and rewritten atomically as `NAME=value` lines in fixed order, keeping
the other keys' values and dropping comments, `export`, quotes, a byte order mark and repeated lines: a stale `.config.<pid>.tmp` of this
process id is removed, a temporary owner-only (0600) file is written through its handle, synced and renamed over `config` (a symlinked
`config` becomes a regular file), then the directory is synced (Unix, best effort). The owner-only modes are Unix only: on Windows the
directory, the lock and the file get no permissions of their own and keep the ACLs they inherit. Data: `ConfigData` after the write.
Stages: `not_applicable`, `not_applicable`, persistence `applied`. Errors: `invalid_arguments`, `configuration_invalid`,
`configuration_unwritable` (a proposed new code, awaiting approval), `internal_error`.

## `config show` (implemented, unit 4)

```text
mostro-cli config show [--output human|json]
```

Canonical name `config.show`; read-only, needs nothing. Reports the file's path and the four values stored in it, each null when absent
(every value null without a file; the path null when no data directory can be determined or its path is not UTF-8). Options and environment
are not reported. The file is read strictly, so one it cannot reach is `configuration_invalid` with the hint, never a set of nulls. Stages:
`not_applicable` x3. Errors: `invalid_arguments`, `configuration_invalid`, `internal_error`. The values shown are only ones the commands
accept (a key, relay URLs, a number, `nip44`); this is not a secrecy promise.

## Human output (implemented)

`Config file: <path>` (`Config file: unknown (no data directory, or its path is not UTF-8)` for a null path), then one `<NAME>: <value>`
or `<NAME>: not set` line per setting; `--verbose` adds `verbose: config.set ok` or `verbose: config.show ok`.
