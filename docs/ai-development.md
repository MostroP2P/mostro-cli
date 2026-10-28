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

The cross-project roadmap is [plan-aiReadyMostroCli.md](../../plan-aiReadyMostroCli.md).
It sits beside the `mostro-cli` and `mcp` clones, outside both repositories, and
is not tracked by Git; copy it next to the clones on a new machine. Feature
specifications refine bounded parts of it rather than copying the entire
roadmap. The first pilot is partly implemented (T001-T054 complete, T055 next, Windows run blocked);
earlier persistence fixes and their test results remain prior work.

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

## Resume On Another Linux/WSL Laptop

This is a development handoff, not a trading-identity migration or a compatible
CLI/MCP release pair. At the 2026-10-28 checkpoint, T001-T054 are complete, the
baseline and foundation gates are closed with recorded exceptions, US1 (machine
listing), US2 (human presentation) and US3 (offline discovery and stable errors)
are implemented and accepted on Linux, and T055 is next; the native Windows run (T052, SC-005) is still blocked. Follow the [task list](../specs/001-order-list-json/tasks.md) and
[test evidence](../tests/TEST_SUMMARY.md) if work has advanced since then. The
JSON pilot and full baseline gate remain unverified; the declared MSRV is 1.94
(raised from 1.86 to match the locked graph), and regtest compatibility is
verified only for listing, order creation and cancellation.

### Install The Local Tools

For WSL, install Ubuntu under WSL2 and the Microsoft WSL extension in Windows
VS Code. Run every shell command below inside Linux, not PowerShell or Git Bash.
Keep the checkout under the Linux home directory rather than `/mnt/c`. For native
Linux, install [VS Code](https://code.visualstudio.com/) normally. Install GitHub
Copilot and rust-analyzer in the active Linux/WSL extension host, sign into your
GitHub account, and trust only your own reviewed workspace.

On Ubuntu/Debian, run these package commands yourself; sudo may request your
local password. Other distributions need equivalent development packages.

```bash
sudo apt update
sudo apt install -y build-essential cmake pkg-config libssl-dev git curl ca-certificates python3 python3-venv pipx
pipx install uv
pipx ensurepath
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
```

Install Rust using the official [rustup installer](https://rustup.rs/) if rustup
is not already available. Then install the checkpoint toolchain and Python:

```bash
rustup toolchain install 1.97.0 --profile minimal --component clippy --component rustfmt --component rust-analyzer
uv python install 3.11
uv tool install --python 3.11 specify-cli==1.2.0
```

The CLI's toolchain file may additionally download its declared rust-docs and
wasm target components. Do not change the pin or lockfiles to hide an unavailable
toolchain. Docker Compose and jq are only needed for later, explicitly authorized
regtest work, not this handoff or T011. No live node, wallet or identity is needed.

### Clone Both Repositories

These are the configured origins for this handoff. The publishing laptop must
push both `develop` branches before these commands can retrieve the checkpoint.

```bash
mkdir -p "$HOME/src/mostro-dev"
cd "$HOME/src/mostro-dev"
git clone --branch develop https://github.com/MostroP2P/mostro-cli.git mostro-cli
git clone --branch develop https://github.com/MostroP2P/mcp.git mcp
cd mostro-cli
git status --short --branch
git log -1 --oneline
git -C ../mcp status --short --branch
git -C ../mcp log -1 --oneline
```

For existing clean clones, use `git switch develop` and
`git pull --ff-only origin develop` in each repository instead of cloning over
them. Preserve local work first; never reset it to force the handoff.

### Restore Spec Kit And Open The Workspace

The checked-in scaffold already contains the skills, templates, Python scripts,
constitution, specification, plan, tasks, research and contracts. Do not run
`specify init`, regenerate the feature, install optional extensions or create a
feature-named branch merely to resume.

From the CLI repository root:

```bash
uv venv --python 3.11 .venv
source .venv/bin/activate
specify check
SPECIFY_FEATURE_DIRECTORY=specs/001-order-list-json python .specify/scripts/python/check_prerequisites.py --json --require-tasks --include-tasks
python .specify/scripts/python/resolve_template.py constitution-template --json
python .specify/scripts/python/check_prerequisites.py --json --require-tasks --include-tasks
code ../mcp/mostro-cli-mcp.code-workspace
```

The explicit feature-directory invocation writes the ignored local
`.specify/feature.json` pointer; the second prerequisite invocation checks that
selection without the environment override. Expect FEATURE_DIR to resolve to
this clone's `specs/001-order-list-json`, not a Windows path. `.venv` is also
ignored. Activate it in each new CLI terminal; alternatively run the scripts
with `uv run --no-project --python 3.11 python` instead of `python`.

In WSL, `code` must open a WSL-connected window. Both folders should appear in
Explorer. If `code` is unavailable, open the workspace using VS Code's Open
Folder/Workspace command in the Linux/WSL window. The workspace's Windows-only
terminal preferences do not configure Linux terminals. No MCP server starts
automatically; opening the repository is not consent to execute its tools.

### Verify The Destination Checkout

Run from the CLI root before changing code:

```bash
rustc -Vv
cargo -V
cargo test --locked --test cli_functions new_order_preflight_process
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
```

At this checkpoint Windows passed 11 preflight regressions and 198 total tests,
plus fmt, Clippy and build. These are reference counts, not proof of a Linux run.
Record the destination results and stop on unexpected failures. The tests use
isolated fixtures; do not replace them with live trades or real identity data.

Check MCP separately using the same explicitly selected development toolchain:

```bash
cd ../mcp
cargo +1.97.0 fmt --all -- --check
cargo +1.97.0 clippy --locked --all-targets --all-features -- -D warnings
cargo +1.97.0 test --locked
cargo +1.97.0 build --locked
cd ../mostro-cli
```

MCP's handoff checks on Windows passed formatting and binary build. Unit tests
passed 40 cases and failed 7 in the existing `/bin/sh` fixture launcher; full
tests and all-target Clippy could not compile the Unix-only permissions fixture.
Linux is the intended runtime for these fixtures, but a passing run must still
be recorded. These existing failures are not repaired by the documentation handoff.
Do not run ignored regtest tests or `regtest/devenv.sh clean` as setup. Keep
`MOSTRO_MCP_ALLOW_WRITES` unset. T005 intentionally rejects closed-stdin new-order
confirmation, so the existing MCP cannot create an order by relying on the old
EOF-approval bug; do not bypass that fix or assume write compatibility.

### Configure Git Access And Signing

Authenticate to GitHub on the destination through VS Code or your chosen Git
credential manager. Never put tokens in clone URLs, shell history or repository
files. Set your own Git author name/email. For signed development commits,
configure a signing key on that laptop; Git configuration and SSH keys are not
transferred by cloning. With an SSH signing key, set `gpg.format=ssh`,
`user.signingkey` to its public-key path and `commit.gpgsign=true`. Register the
public key as a signing key in GitHub. Local signature verification additionally
needs an allowed-signers file; do not mistake signing success for verified trust.
Enter key passphrases directly in your terminal, never in Copilot chat.

### Resume Prompt

In Copilot Agent chat with both repositories open:

```text
Continue mostro-cli on develop, implementing T055 only (the Windows run remains blocked) from
specs/001-order-list-json/tasks.md. Read AGENTS.md and the constitution first,
then the existing plan and tests/TEST_SUMMARY.md. T001-T054 are complete;
the baseline and foundation gates are closed with recorded exceptions and the JSON pilot
is only partly implemented (US3 discovery and the polish gates remain).
A test ignored with "red until T0xx" is removed from the ignore list when its task lands.
Use the existing Spec Kit feature; do not regenerate its artifacts.
Use .venv/bin/python (or uv run --no-project --python 3.11 python) for its scripts.
Read the cross-repository roadmap in ../plan-aiReadyMostroCli.md (outside both repositories).
Preserve local edits. Reproduce any defect with isolated fixtures, then make
the bounded repair and run the focused and required gates. Do not access real
identities, publish trades, retry uncertain writes, change MCP code, or perform
Git mutations without fresh authorization. Record Linux results separately
from the earlier Windows evidence.
```

Git transfers source and committed documentation, not chat history, session
memory, installed tools, VS Code sign-in, build caches or machine-local settings.
Do not copy `.env`, `.mcli`, SQLite identity databases, private keys or regtest
volumes as part of this development setup. The tracked task list and evidence
are the handoff; keep them current after each completed task.
