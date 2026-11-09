# Data Model: Node Settings From A Config File

## The file

`$MCLI_DIR/config`, else `<home>/.mcli/config`. Example:

```bash
# ~/.mcli/config   (chmod 600)
MOSTRO_PUBKEY=npub1...
RELAYS=wss://relay.example,wss://other.example
POW=0
TRANSPORT=nip44
```

| Key | Accepted value | Read by (unit 1) |
| --- | --- | --- |
| `MOSTRO_PUBKEY` | a public key, hex or npub | every JSON-capable networked command |
| `RELAYS` | comma-separated relay URLs (`ws://` or `wss://`), at least one | every JSON-capable networked command |
| `POW` | an integer 0 to 255 (the floor of the work mined) | the commands that send a request |
| `TRANSPORT` | `nip44` (skips the protocol check, as `-t nip44` does) | the commands that send a request |

## Reads

| Case | `read_config` (networked commands) | `read_config_strict` (`config set`, `config show`) |
| --- | --- | --- |
| no data directory, no `config`, a data directory that is a file | no file | no file |
| a data directory the process may not search or whose path cannot be resolved (a symlink loop, a name too long) | no file | `Unreadable` |
| `config` is not a regular file, cannot be read, is not UTF-8 | `Unreadable` | `Unreadable` |
| a line refused | `Malformed`, `KeyNotAllowed` or `InvalidValue` with the line | the same |

## `ConfigFileError`

| Variant | Text after `<path>: ` |
| --- | --- |
| `Unreadable` | `not a readable UTF-8 text file` |
| `Malformed { line }` | `line N is not KEY=VALUE` |
| `KeyNotAllowed { line }` | `line N sets a key other than MOSTRO_PUBKEY, RELAYS, POW or TRANSPORT` |
| `InvalidValue { line }` | `line N has a value its setting refuses` |

On the JSON-capable path a refusal is `configuration_invalid` (exit 1, stages unchanged: nothing published or written) and stderr has
exactly one line `hint: <path>: <text>`; in human mode it is followed by `error: The node configuration is invalid.`

## Proposed (units 2 to 4; the new code and keyword await the maintainer's approval)

- `ConfigData` (data of `config.set` and `config.show`): `path`, `mostro_pubkey`, `relays`, `pow`, `transport`, each a string or null,
  the values as stored in the file.
- `configuration_unwritable` (exit 1, "The configuration file cannot be written, so nothing was changed."): `config.set` only, persistence
  `not_attempted` (no data directory, or the directory or temporary file cannot be created) or `failed` (a write or rename failed; the old
  file is intact).
- Discovery keyword `x-positional: N` on input properties passed as positional arguments (additive within v1).
