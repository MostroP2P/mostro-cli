# Feature Specification: Invoice From Standard Input

**Feature**: `010-invoice-stdin`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-05

**Status**: Implemented and verified on Linux against the scripted node (slices 1-3, 2026-11-05; see `tests/TEST_SUMMARY.md`); CI is checked after
the push. Not run against a real node: reading the invoice changes nothing the node sees. Progress is tracked only in [tasks.md](tasks.md)

**Input**: An end-to-end harness (Mortsom) drives `mostro-cli` as a subprocess. An invoice on the command line is visible in the process list and
in logs of the harness. `--invoice -` reads it from standard input instead, on every command that takes `--invoice`: `order add-invoice`,
`bond add-invoice`, `order take-sell` and `order new`. Builds on [add invoice](../005-invoice-json/spec.md), [order take](../004-order-take-json/spec.md)
and [order new](../003-order-new-json/spec.md); their behavior with a literal invoice does not change.

## Clarifications

### Session 2026-11-05 (the harness contract)

- Q: What may standard input hold? -> A: At most 8 KiB (8192 bytes), valid UTF-8, and exactly one non-empty line once ASCII whitespace is trimmed
  from both ends (a trailing newline or CRLF is fine). Empty input, a second line, more than 8192 bytes or invalid UTF-8 is `invalid_arguments`
  (exit 2) before any effect. To tell an oversize input the program reads at most 8193 bytes and stops; it never reads to the end of a larger one.
- Q: And then? -> A: The line is validated exactly like a literal `--invoice`: BOLT-11 or Lightning address where the command accepts it, BOLT-11
  only for `bond add-invoice`; the commands' own size and character rules apply.
- Q: When does the read end? -> A: At end of input (amended 2026-11-05 after review): the writer must close standard input after the line;
  a second line is refused, not ignored. A terminal never ends the input by itself, so `-` with a terminal as stdin is `invalid_arguments` at
  once, without reading (`std::io::IsTerminal`). The help of the four options and the discovery description say so.
- Q: Confirmation? -> A: Stdin carries the invoice, so it cannot also carry the answer to the question. `--invoice -` requires `--yes` in human
  mode as well; without it the command fails with `invalid_arguments` before stdin is touched. JSON keeps requiring `--yes`. Stdin is never read
  for a confirmation and end of input is never approval.
- Q: Secrecy? -> A: As before, the invoice never appears in a diagnostic, preview, error or JSON document, whatever its source. The program never
  echoes what it read, valid or not.
- Q: Discovery? -> A: The generated input schema of each of the four commands marks `--invoice` with `"x-stdin": "-"` and says so in its
  description.
- Q: Is a literal `-` ever an invoice? -> A: No. `-` is not a valid invoice for any command, so the meaning is unambiguous.

## Requirements

- **FR-001**: `--invoice -` on `order add-invoice`, `bond add-invoice`, `order take-sell` and `order new` reads the invoice from standard input;
  any other value is the invoice itself, unchanged.
- **FR-002**: The read is bounded and strict as clarified; every rejection is `invalid_arguments` (exit 2) with nothing published, written or
  echoed.
- **FR-003**: Without `--yes`, `--invoice -` is `invalid_arguments` in both modes and stdin is not read.
- **FR-004**: After the read, validation and every later step are those of a literal `--invoice`.
- **FR-005**: The invoice from stdin never reaches stdout, stderr or a JSON document; the secret sentinel tests cover it.
- **FR-006**: `schema` documents the stdin form in the input schemas (`x-stdin`, description), regenerated `docs/schemas/discovery-v1.json`, drift
  tests.

## Success Criteria

- **SC-001**: For each of the four commands, `--invoice - --yes --output json` with the invoice on stdin publishes exactly that invoice to the
  scripted node and reports the same outcome as the literal form.
- **SC-002**: Empty, multi-line, oversize and non-UTF-8 inputs each yield one `invalid_arguments` document, exit 2, nothing published and the store
  byte-identical, for each command.
- **SC-003**: Without `--yes`, a run with an open, never-written stdin ends at once with `invalid_arguments` (it does not wait for input).
- **SC-004**: No stream of any run above contains the invoice.
- **SC-005**: The four input schemas carry the marker; no other option does.

## Out of scope

Reading any other option from stdin, reading the invoice from a file or an environment variable, a confirmation on the terminal device.
