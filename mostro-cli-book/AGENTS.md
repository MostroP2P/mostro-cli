# Editing this book (for AI agents and humans)

This book documents the JSON interface of mostro-cli. It must never say anything the binary
does not do. Follow these rules.

## Sources of truth, in order

1. The binary: `mostro-cli --output json schema` (copied to `schemas/discovery.json`).
2. The generated per-command schemas in `schemas/*.input.json` and `schemas/*.output.json`.
3. The CLI's contract folders `../specs/NNN-*/contracts/` (`cli.md`, `examples.json`) and
   data models `../specs/NNN-*/data-model.md`.
4. The CLI source: `../src/outcome/` (envelope, codes, data types) and `../src/cli/registry.rs`
   (command names).

If a page disagrees with 1 or 2, the page is wrong.

## Layout and conventions

- One page per command in `src/commands/`, file name = canonical name with `.` and `-`
  replaced by `_` (`order.take-sell` -> `order_take_sell.md`). Every command page has the
  same sections, in this order: summary table, **Invocation**, **Input**, **Behavior**,
  **Success data**, **Outcomes** (status/code/stages table), **Errors**, **Warnings**,
  **Examples**, **Next**. Copy an existing page when adding a command.
- Examples are never written inline in a page. Each is a file in `examples/<command>/<case>.json`
  holding exactly the stdout of the CLI, listed in `examples/index.json` with its `argv`,
  `exit_code`, `title` and `source`. Pages pull it in with
  `{{#include ../../examples/<command>/<case>.json}}` inside a ```` ```json ```` fence.
- `source.kind` is `fixture` (from the CLI's tests; refresh with `scripts/import_fixtures.py`,
  never edit by hand), `captured` (real output of the binary) or `authored` (written for the
  book; must validate). Never present an authored example as captured.
- Field tables use the columns `Field | Type | Meaning`. Types use the vocabulary of
  `src/types.md` (`decimal string`, `uuid`, `hex64`, `rfc3339`, `... or null`).
- Codes are written in backticks exactly as on the wire: `node_refused`, `pay_hold_invoice`.
- Keep the error and warning catalogues (`src/errors.md`, `src/warnings.md`) and the
  per-command lists in sync with the `enum` lists of the output schemas.

## After any change

```sh
python3 scripts/validate_examples.py     # every example validates, every file is listed
python3 scripts/gen_examples_page.py     # refresh src/reference/examples.md
mdbook build                             # no broken include or link
```

## When the CLI changes

1. `MOSTRO_CLI=<binary> scripts/refresh_schemas.sh` and read the `git diff` of `schemas/`.
2. `python3 scripts/import_fixtures.py --cli-repo <cli checkout>`.
3. A new command: add a page, examples, an entry in `src/SUMMARY.md` and `src/commands/README.md`.
   A new error or warning code: add it to the catalogues and to every page whose schema lists it.
4. Update the version line in `README.md` and `src/introduction.md`.
