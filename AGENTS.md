# Mostro CLI Development Rules

Read the [constitution](.specify/memory/constitution.md) before changing code or
specifications. It is the authoritative source for coding standards, safety,
verification gates, and the `develop` workflow. Do not duplicate those rules here.

## Spec Kit

Use the [AI development guide](docs/ai-development.md) for installation,
Copilot skills, and documentation ownership. Spec Kit uses Python scripts;
do not bypass PowerShell execution policy or add the optional Git extension.

The active feature is [order-list JSON](specs/001-order-list-json/spec.md).
It is implemented and verified on Linux with loopback relays: all of T001-T055 are
checked in the [task list](specs/001-order-list-json/tasks.md), but the native
Windows run (part of T052, SC-005) is still blocked, so the pilot is not accepted
for release. The next work is that Windows run. A follow-on draft exists,
[order show](specs/002-order-show-json/spec.md) (spec only: two open clarifications,
no plan or tasks); `order new` and `order take` are later specs. Continue from existing
regression evidence and roadmap gates; do not restart or overwrite prior work.

## Current Code References

Use [architecture](docs/architecture.md), [commands](docs/commands.md),
[database](docs/database.md), and [tests](tests/README.md) for current code.
The cross-repository refactor plan is
[plan-aiReadyMostroCli.md](../plan-aiReadyMostroCli.md), kept beside the two
clones and outside both repositories. Only the order-list pilot modules exist
so far; the other planned modules are not yet implemented.
