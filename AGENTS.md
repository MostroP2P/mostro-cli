# Mostro CLI Development Rules

Read the [constitution](.specify/memory/constitution.md) before changing code or
specifications. It is the authoritative source for coding standards, safety,
verification gates, and the `develop` workflow. Do not duplicate those rules here.

## Spec Kit

Use the [AI development guide](docs/ai-development.md) for installation,
Copilot skills, and documentation ownership. Spec Kit uses Python scripts;
do not bypass PowerShell execution policy or add the optional Git extension.

The active feature is [order-list JSON](specs/001-order-list-json/spec.md).
It is implemented and verified on Linux with loopback relays and, in CI, on a
GitHub-hosted native Windows runner: all of T001-T055 are checked in the
[task list](specs/001-order-list-json/tasks.md). Live relays and nodes are not
tested. Three follow-on features are implemented and verified on Linux and the
CI-hosted Windows runner against a scripted fake daemon, and probed on a local
regtest node (mostrod v0.19.3, 2026-11-01; bonded and Cashu nodes not covered): [order info](specs/002-order-info-json/spec.md)
([tasks.md](specs/002-order-info-json/tasks.md)) and
[order new](specs/003-order-new-json/spec.md)
([tasks.md](specs/003-order-new-json/tasks.md)) and
[order take](specs/004-order-take-json/spec.md)
([tasks.md](specs/004-order-take-json/tasks.md)); the add-invoice commands
([spec](specs/005-invoice-json/spec.md), [tasks.md](specs/005-invoice-json/tasks.md)),
`message list` ([spec](specs/006-message-list-json/spec.md), [tasks.md](specs/006-message-list-json/tasks.md))
and `order fiat-sent`, `order release` and `order cancel` ([spec](specs/007-trade-steps-json/spec.md), [tasks.md](specs/007-trade-steps-json/tasks.md)) and range children with `order sync` ([spec](specs/008-range-children/spec.md), [tasks.md](specs/008-range-children/tasks.md))
are verified against the scripted node only; `order add-invoice`, `message list` and the trade steps (except a range order) were probed on a real regtest node, range children (a sell range order) were probed too, `bond add-invoice` was not.
The newest features are `identity init` and `identity show` ([spec](specs/009-identity-json/spec.md), [tasks.md](specs/009-identity-json/tasks.md)),
local commands verified on Linux with isolated stores (no node involved), and `--invoice -`, the invoice read from standard input by the four
commands that take one ([spec](specs/010-invoice-stdin/spec.md), [tasks.md](specs/010-invoice-stdin/tasks.md)), verified against the scripted node.
After them: `order mine` ([spec](specs/011-order-mine/spec.md)), `order new --expires-in` ([spec](specs/012-order-expires-in/spec.md)),
`dispute open` ([spec](specs/013-dispute-open/spec.md)), which replaces `dispute`, verified on Linux against isolated
stores and the scripted node only.
Continue from existing regression evidence and roadmap gates; do not restart or overwrite prior work.

## Current Code References

Use [architecture](docs/architecture.md), [commands](docs/commands.md),
[database](docs/database.md), and [tests](tests/README.md) for current code.
The cross-repository refactor plan is
[plan-aiReadyMostroCli.md](../plan-aiReadyMostroCli.md), kept beside the two
clones and outside both repositories. Only the `order list`, `order info`, `order new`, the two
`order take-*`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `order mine`,
`dispute open`, `bond add-invoice`, `message list`, `identity init`, `identity show` and `schema` modules are migrated; the other planned modules are not yet implemented.
