# Mostro CLI Development Rules

Read the [constitution](.specify/memory/constitution.md) before changing code or
specifications. It is the authoritative source for coding standards, safety,
verification gates, and the `develop` workflow. Do not duplicate those rules here.

## Spec Kit

Use the [AI development guide](docs/ai-development.md) for installation,
Copilot skills, and documentation ownership. Spec Kit uses Python scripts;
do not bypass PowerShell execution policy or add the optional Git extension.

The active feature is [order-list JSON](specs/001-order-list-json/spec.md).
It is a specification, not completed implementation. Continue from existing
regression evidence and roadmap gates; do not restart or overwrite prior work.

## Current Code References

Use [architecture](docs/architecture.md), [commands](docs/commands.md),
[database](docs/database.md), and [tests](tests/README.md) for current code.
The cross-repository refactor plan lives in the MCP repository as
`plan-aiReadyMostroCli.prompt.md`; planned modules are not yet implemented.
