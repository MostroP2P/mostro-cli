# Mostro CLI Development Rules

Read the [constitution](.specify/memory/constitution.md) before changing code or
specifications. It is the authoritative source for coding standards, safety,
verification gates, and the `develop` workflow. Do not duplicate those rules here.

## Spec Kit

Use the [AI development guide](docs/ai-development.md) for installation,
Copilot skills, and documentation ownership. Spec Kit uses Python scripts;
do not bypass PowerShell execution policy or add the optional Git extension.

The active feature is [order-list JSON](specs/001-order-list-json/spec.md).
It is partly implemented: T001-T054 are complete and T055 is next; the native Windows run (T052, SC-005) is still blocked, per the
[task list](specs/001-order-list-json/tasks.md). Continue from existing
regression evidence and roadmap gates; do not restart or overwrite prior work.

## Current Code References

Use [architecture](docs/architecture.md), [commands](docs/commands.md),
[database](docs/database.md), and [tests](tests/README.md) for current code.
The cross-repository refactor plan is
[plan-aiReadyMostroCli.md](../plan-aiReadyMostroCli.md), kept beside the two
clones and outside both repositories. Only the order-list pilot modules exist
so far; the other planned modules are not yet implemented.
