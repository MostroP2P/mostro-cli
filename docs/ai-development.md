# AI Development With Spec Kit

## Installed Setup

This repository uses Spec Kit 1.2.0 with the GitHub Copilot skills integration
and portable Python scripts. It is development tooling, not a Cargo dependency.
MCP remains a separate project and will adopt its own feature workflow during
the JSON integration phase.

Prerequisites: Python 3.11 or newer, `uv`, and VS Code with GitHub Copilot.
From this repository root, install the pinned CLI and check its availability:

```sh
uv tool install specify-cli==1.2.0
specify check
```

The scaffold is already installed. Do not rerun `specify init --force` as a
routine setup step: it can replace managed files. A deliberate reinstall or
upgrade must preserve existing edits, review generated changes, and retain the
`copilot` integration with `--script py`. No optional extensions are installed.

The deprecated `taskstoissues` skill and unused PowerShell helpers are deliberately
omitted, with their entries removed from the installer manifests. Spec Kit 1.2.0
may restore them during a refresh, even with `--script py`; review and reapply
this cleanup after upgrades. Keep the remaining manifests, configuration,
workflow, templates and Python scripts in Git for reproducible setup and safe
upgrade detection. Do not replace recorded hashes for customized files. Local
feature selection and Python caches remain ignored.

Python scripts avoid changing this Windows machine's PowerShell execution policy
and can also run on Linux. Use the interpreter returned by `python --version`;
if `python` is unavailable, `uv run --no-project python` can run the same scripts.

## Sources Of Truth

| Artifact | Owns |
| --- | --- |
| [Constitution](../.specify/memory/constitution.md) | Coding standards, safety, and verification rules |
| [Agent entrypoint](../AGENTS.md) | Navigation and task routing |
| [Pilot specification](../specs/001-order-list-json/spec.md) | Required user-visible behavior and acceptance criteria |
| Feature plans and tasks | Technical decisions, dependencies, and recorded execution |
| Existing docs and Rustdoc | Implemented architecture and APIs |
| Generated command references and schemas | Actual command/input/output contracts once implemented |

The MCP repository's `plan-aiReadyMostroCli.prompt.md` remains the cross-project
roadmap. Feature specifications refine bounded parts of it rather than copying
the entire roadmap. The first pilot is not implemented; earlier persistence
fixes and their test results remain prior work.

## Copilot Workflow

Run these as Copilot chat skills, not terminal commands. In a multi-root workspace,
explicitly target `mostro-cli`. Skills live under `.github/skills/`.

1. `/speckit-clarify` reviews ambiguity in the existing order-list specification.
2. `/speckit-plan` creates its technical plan against the constitution and current code.
3. `/speckit-tasks` creates dependency-ordered work, including required tests.
4. `/speckit-analyze` checks agreement between requirements, plan, and tasks.
5. `/speckit-implement` performs an explicitly scoped phase after its prerequisites pass.
6. `/speckit-converge` assesses remaining gaps; it is not a replacement for tests.

Use `/speckit-constitution` for approved governance amendments and
`/speckit-specify` for a new bounded feature. Do not regenerate the current pilot
merely to resume work. Current Phase 1 baseline gates still govern implementation.

Keep `develop`; feature directory names are independent of Git branches. The
optional Git extension is deliberately absent. `.specify/feature.json` selects
the active feature, so no branch renaming or persistent environment variable is
needed. Script output fields named `BRANCH` can identify the feature rather than
the actual Git branch; verify the latter with Git when it matters.

The active-feature pointer is intentionally ignored by Git. On a fresh clone,
tell the skill to use the existing `specs/001-order-list-json` directory, rather
than running `/speckit-specify` again. For a terminal-only check in Bash, select
it explicitly for that invocation:

```sh
SPECIFY_FEATURE_DIRECTORY=specs/001-order-list-json python .specify/scripts/python/check_prerequisites.py --paths-only --json
```

## Setup Checks

```sh
python .specify/scripts/python/resolve_template.py constitution-template --json
python .specify/scripts/python/check_prerequisites.py --paths-only --json
```

The ordinary prerequisite check requires a technical plan. Until `/speckit-plan`
runs, a missing-plan error is expected; do not create an empty plan to bypass it.
Checked specification-quality items are documentation review, not evidence that
the feature works. Record code-test results separately when implementation lands.
