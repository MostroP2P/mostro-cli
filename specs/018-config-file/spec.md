# Feature Specification: Node Settings From A Config File

**Feature**: `018-config-file`

**Working Branch**: `feat/config-file-read`, `feat/config-file-legacy` and `feat/config-set`, the first three of four stacked branches (feature directory naming does not change Git branches)

**Created**: 2026-11-08

**Status**: Units 1 to 3 implemented and verified on Linux against loopback relays, the scripted node and isolated sandboxes (see
`tests/TEST_SUMMARY.md`): every networked command, JSON-capable or human-only, reads the file, and `config set` writes it (examples in
[contracts/examples.json](contracts/examples.json)). Proposed: `config show` (unit 4). Progress is tracked only in [tasks.md](tasks.md)

**Input**: issue #244. An agent drove a whole trade with mostro-cli 0.17.0 and repeated `-m <npub> -r <relays> -p <bits>` on every
call: coding agents and MCP hosts run each command in a fresh shell, so an `export` does not survive from one call to the next. The node
settings should come from a file in the data directory, after the options and the environment.

## Clarifications

### Session 2026-11-08 (maintainer decisions)

- Q: Scope? -> A: Minimal: reading the file, `config set <key> <value>` and `config show` (JSON-capable). `config show` reports the
  file's path and the values stored in it; no per-value source attribution and no warning when a variable overrides the file. No extra
  codes or warnings unless strictly needed, and no legacy aliases.

The delivery in four units, the new error code and the new discovery keyword are design proposals awaiting the maintainer's approval;
see [plan.md](plan.md).

## Requirements

- **FR-001 Precedence**, per setting, as the issue's lookup `env::var(name).ok().or_else(|| file.get(name))`: the root option (`-m`,
  `-r`, `-p`, `-t`), even empty; else the environment variable when it is set, even empty; else the config file; else nothing. Empty,
  invalid and missing values keep their handling: an empty `-m` or `-r`, or a set but empty `MOSTRO_PUBKEY` or `RELAYS`, is
  `configuration_missing`; an empty `POW` is 0 and an empty `TRANSPORT` means detect; an invalid value is `configuration_invalid`. A set
  variable whose value is not valid UTF-8 counts as set but empty: it still beats the file and, as before the file existed, is blank.
- **FR-002 No file**: without a config file every command's behavior, outcomes and JSON are unchanged; only the guidance text of the
  legacy missing-setting messages differs. The existing suites pass unchanged.
- **FR-003 Location**: `config` in the data directory: a non-blank `MCLI_DIR`, else `<home>/.mcli` (on Windows the profile folder,
  normally `%USERPROFILE%\.mcli\config`). With neither there is no file.
- **FR-004 Format**: UTF-8 text, LF or CRLF, one leading byte order mark (U+FEFF) ignored. Blank lines and lines starting with `#` (after
  leading whitespace) are skipped; an `export` followed by whitespace is dropped; a line is split at its first `=`, key and value are
  trimmed, one pair of matching `"` or `'` quotes around the whole value is removed and the value is trimmed again. No inline comments,
  escapes or expansion. An empty value means unset; a repeated key keeps its last line. The README `env.sh` parses as is.
- **FR-005 Keys and values**: the keys are exactly `MOSTRO_PUBKEY`, `RELAYS`, `POW` and `TRANSPORT`. The whole file is refused on another
  key, a line without `=`, a value its setting refuses (a public key in hex or npub; at least one relay URL, each `ws://` or `wss://`; an
  integer 0 to 255; exactly `nip44`; a control character anywhere), or a `config` that is not a regular file, cannot be read or is not
  UTF-8. A refusal names the line and the kind of problem, never the content. Two reads: lenient for the networked commands (any error
  looking the file up, such as not found, not a directory, permission denied, a symlink loop or a name too long, means no file, so a
  sandbox that hides the home directory or a data directory that cannot be used keeps working with options and variables, as before the
  file existed) and strict for `config set` and `config show` (only not found and not a directory mean no file; any other lookup error is
  a refusal).
- **FR-006 Reading** creates nothing, reads only the four names, never writes into the process environment, and happens once per run,
  before identity, store or network access.
- **FR-007 JSON-capable networked commands** (unit 1): `order list` (after filter validation), `order info` (after id validation),
  `order new`, both takes, `order add-invoice`, `bond add-invoice`, the three steps, `dispute open`, `order rate`, `message list` and
  `order sync` read the file first. A refused file is `configuration_invalid` (exit 1, nothing sent), even when the options supply every
  value, with one stderr line `hint: <path>: <problem>` in both modes. `schema`, `identity init`, `identity show` and `order mine` never
  read it.
- **FR-008 Legacy commands** (unit 2): the same precedence through one lookup (`settings::var`: a variable that is not present is
  answered by the file, whose values never enter the environment); a refused file stops them before `mcli.db` exists or `ADMIN_NSEC` is
  read, with `Invalid config file <path>: <problem>` and exit 1. The RPC commands never read it.
- **FR-009 `config set <key> <value>`** (unit 3, implemented), JSON `config.set`: see [contracts/cli.md](contracts/cli.md).
- **FR-010 `config show`** (unit 4, proposed), JSON `config.show`: see [contracts/cli.md](contracts/cli.md).
- **FR-011 Windows**: the same location rules with the profile folder, no POSIX modes; verified only by CI after the branches are pushed.

## Success Criteria

- **SC-001**: With only the file, `order list` and `order new --yes` run without `-m`, `-r` or `-p` (unit 1), and so does `listdisputes`
  (unit 2).
- **SC-002**: An option beats a set variable, which beats the file, measured by the connections of loopback relays (JSON path in unit 1,
  legacy path in unit 2).
- **SC-003**: An `ADMIN_NSEC` line gives `configuration_invalid` (or `Invalid config file`) at every entry point, with nothing echoed,
  created or published.
- **SC-004**: `config set` round-trips with `order list` and with `config show` (both to be verified in unit 4, see [tasks.md](tasks.md)).
- **SC-005**: The existing per-command fixtures are unchanged and `schema_version` stays `"1"`.
