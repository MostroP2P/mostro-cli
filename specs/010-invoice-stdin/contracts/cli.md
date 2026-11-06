# CLI Contract: Invoice From Standard Input

Amends the grammar of [add invoice](../../005-invoice-json/contracts/cli.md), [order take](../../004-order-take-json/contracts/cli.md) and
[order new](../../003-order-new-json/contracts/cli.md); everything else in those contracts holds.

## Grammar

```text
mostro-cli order add-invoice --order UUID --invoice - --yes [--output human|json] < invoice.txt
mostro-cli bond add-invoice  --order UUID --invoice - --yes [--output human|json] < invoice.txt
mostro-cli order take-sell   --order UUID --invoice - --yes [--amount FIAT] [--output human|json] < invoice.txt
mostro-cli order new ... --invoice - --yes [--output human|json] < invoice.txt
```

`--invoice -` (also `--invoice=-`) reads the invoice from standard input. `--yes` is required in both modes: without it the result is
`invalid_arguments` (exit 2) and stdin is not read. A literal `--invoice VALUE` is unchanged, including the human prompt when `--yes` is absent,
except for the reason of its refusal (see Errors).

## Standard input

At most 8192 bytes of UTF-8 holding exactly one non-empty line once ASCII whitespace is trimmed from both ends, read until end of input: the
writer must close standard input after the line. Anything else is `invalid_arguments`, exit 2, before any effect: nothing published, nothing
written, nothing echoed. A terminal as stdin is refused the same way at once, without reading. The line is then validated as a literal value.

## Discovery

`mostro-cli schema` lists the property `invoice` of the four commands with `"x-stdin": "-"` and a description naming the stdin form.

## Errors

No new code. The stdin failures are `invalid_arguments`; the error message is the fixed text of that code and never contains what was read.
A refused content, like any invoice value the program refuses, carries `"reason": "invalid_invoice"`; the form refused for a missing
`--yes` or a terminal carries none (amended 2026-11-05 after review).
