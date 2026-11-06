# Feature Specification: The User's Own Orders From The Local Store

**Feature**: `011-order-mine`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux with isolated stores (see `tests/TEST_SUMMARY.md`); no node is involved. Progress is tracked only
in [tasks.md](tasks.md)

**Input**: An end-to-end harness drives the CLI as a subprocess and needs the ids of the orders the local store holds (the ones it made, took or
adopted with `order sync`, range children included) without asking the node. Builds on [identity](../009-identity-json/spec.md) (local,
read-only) and [order sync](../008-range-children/spec.md); none of their documents change.

## Clarifications

### Session 2026-11-05 (the harness contract, with defaults open to veto)

- Q: What is listed? -> A: One entry per row of the local `orders` table: id, side, local status, trade index and trade public key. Read-only,
  no network, no Mostro configuration, no `--yes`.
- Q: Where does the trade index come from? -> A: The table stores the trade key, not its index. The index is found by deriving the keys of the
  recorded indexes, newest first, exactly as `message list` does (the newest 200); a row whose key is not among them has `trade_index: null`,
  and the run carries the existing warning `trade_keys_limited` when older keys were not derived. A row without a usable key has
  `trade_pubkey: null` and `trade_index: null`. Both are decimal string or hex otherwise.
- Q: Order? -> A: Newest first: by trade index, highest first (each order or take reserves the next index); rows whose index is unknown come
  after, newest `created_at` first, then by id.
- Q: Errors? -> A: No store or no user row: `identity_missing`. A store that cannot be read, or a row whose id, side or status cannot be read:
  `store_unreadable`. Nothing is created or changed in any case.

## Requirements

- **FR-001**: `mostro-cli order mine [--output human|json]`, canonical `order.mine`, no options; registered, in discovery with
  `required_configuration` `[]`.
- **FR-002**: `data.orders`: `id` (lowercase UUID), `side` (`buy`/`sell`), `status` (the local status), `trade_index` (decimal string or null),
  `trade_pubkey` (64 hex or null), in the order above. An empty store is a success with `orders: []`.
- **FR-003**: Read-only through `store` (a read-only open that creates no file); no relay, node or Mostro configuration.
- **FR-004**: Read stages (`not_applicable` x3); errors `invalid_arguments`, `identity_missing`, `store_unreadable`, `internal_error`; warning
  `trade_keys_limited`. No private key, mnemonic or invoice is serialized.
- **FR-005**: Output schema `docs/schemas/order-mine-v1.json`, regenerated discovery, examples, drift tests.

## Success Criteria

- **SC-001**: After `order new` and a take against the scripted node, `order mine` lists both with their indexes and keys, newest first; after
  `order sync` the adopted child is listed too.
- **SC-002**: Missing and damaged stores fail with their codes and stay byte-identical; no relay is configured in any run.
- **SC-003**: Human and JSON agree; no secret on any stream.
