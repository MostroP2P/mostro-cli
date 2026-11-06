# Data Model: Identity Commands

Both commands return the standard envelope (v1). Neither serializes the mnemonic, a private key, the store path or a database record.

## `identity.init` data

| Field | Meaning |
| --- | --- |
| `identity_pubkey` | the identity public key of the new store (index 0 of the fresh mnemonic), 64 lowercase hex |

## `identity.show` data

| Field | Meaning |
| --- | --- |
| `identity_pubkey` | the identity public key of the store, 64 lowercase hex |
| `last_trade_index` | the last trade index recorded in the store, a JSON integer from `0` to `2147483647`; `0` when none is recorded |

## Outcome table: `identity.init`

| Case | Status | Code | Stages (publication / acceptance / persistence) |
| --- | --- | --- | --- |
| store created | `ok` | | not_applicable / not_applicable / applied |
| `mcli.db` or a companion file exists | `error` | `identity_exists` (new) | not_applicable / not_applicable / not_attempted |
| no data directory can be determined, or the directory or file cannot be created | `error` | `store_unwritable` | not_applicable / not_applicable / not_attempted |
| the file was created but could not be completed (removed again) | `error` | `store_unwritable` | not_applicable / not_applicable / failed |
| invalid arguments (rejected before it ran) | `error` | `invalid_arguments` | not_applicable x3 |

## Outcome table: `identity.show`

| Case | Status | Code | Stages |
| --- | --- | --- | --- |
| identity read | `ok` | | not_applicable x3 |
| no data directory, no `mcli.db`, no user row | `error` | `identity_missing` | not_applicable x3 |
| the store cannot be read, holds no usable mnemonic, a WAL without its index file, an index out of range | `error` | `identity_unreadable` | not_applicable x3 |
| invalid arguments | `error` | `invalid_arguments` | not_applicable x3 |

Exits: 0 on `ok`, 2 on `invalid_arguments`, 1 otherwise. `internal_error` (exit 1) remains possible for both, as for every command. Neither
command emits `uncertain` or a warning.
