# Data Model: Invoice From Standard Input

No output changes. The four commands keep their outcomes, schemas and examples
([add invoice](../005-invoice-json/data-model.md), [take](../004-order-take-json/data-model.md), [new](../003-order-new-json/data-model.md)).

## Input

| `--invoice` value | Meaning |
| --- | --- |
| `-` | read the invoice from standard input (needs `--yes` in both modes) |
| anything else | the invoice itself, as before |

## Standard input

| Input | Result |
| --- | --- |
| one non-empty line after trimming ASCII whitespace, at most 8192 bytes, valid UTF-8, then end of input (the writer closes stdin) | that line is the invoice; validation as for a literal value |
| empty or only whitespace | `invalid_arguments` (exit 2) |
| a second line (an inner `\n` or `\r` after trimming) | `invalid_arguments` |
| more than 8192 bytes (the program stops reading at 8193) | `invalid_arguments` |
| invalid UTF-8 | `invalid_arguments` |
| `--invoice -` without `--yes` | `invalid_arguments`, stdin not read |
| `--invoice -` with a terminal as stdin | `invalid_arguments`, stdin not read (a terminal never ends the input) |

A refused content (empty, blank, several lines, oversize, not UTF-8) carries `"reason": "invalid_invoice"`; `--invoice -` without `--yes`
or with a terminal carries no reason (amended 2026-11-05 after review). Every `invalid_arguments` here has the stages of a command rejected
before it ran (`not_attempted` / `unknown` / `not_attempted`), no data, and
leaves the store byte-identical.

## Discovery

The input schema property `invoice` of `order.add-invoice`, `bond.add-invoice`, `order.take-sell` and `order.new` carries `"x-stdin": "-"` and a
description that names the stdin form; no other property carries `x-stdin`.
