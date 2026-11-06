# Implementation Plan: Invoice From Standard Input

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented (slices 1-3) with Linux evidence against the scripted node.

## Summary

One small dispatch module, `cli/stdin.rs`, turns `--invoice -` into the invoice text before validation; the four dispatchers call it right after
the JSON `--yes` rule and before their input validation. Discovery marks the option from the same module's constant.

## Technical Context

Rust 2021, MSRV 1.94, no new dependency (`std::io::Read::take`, `str::trim_ascii`). Tests: unit tests of the reader in `cli/stdin.rs`, process
tests of the real binary against the scripted node in `tests/invoice_stdin_process.rs` (the harness gains a run with stdin bytes and a run whose
stdin stays open), discovery tests in `cli/schema.rs`, drift of `docs/schemas/discovery-v1.json`.

## Design

1. **Reader** (`cli/stdin.rs`): `STDIN_VALUE = "-"`, `STDIN_LIMIT = 8192`, `STDIN_OPTIONS = ["invoice"]`; `read_line(impl Read)` reads at most
   `STDIN_LIMIT + 1` bytes and returns the one trimmed line or `InvalidStdin`; `invoice(value, yes)` returns the literal value untouched, refuses
   `-` without `yes` before touching stdin, and otherwise reads it. Errors carry nothing that was read.
2. **Dispatch**: `cli/invoice.rs::prepare`, `cli/order_take.rs::outcome` and `cli/order_new.rs::outcome` resolve the invoice after `needs_yes`
   and before `validate`; a failure is `refuse(command, invalid_arguments)`.
3. **Grammar and discovery**: the `--invoice` help of the four commands mentions `-`; `cli/schema.rs::property` adds `"x-stdin": "-"` to the
   options named in `STDIN_OPTIONS`; a test checks that exactly the four commands carry it.

## Risks

- **A hung harness**: a reader that waited for input without `--yes` would block; the `--yes` check comes first and a process test keeps stdin
  open to prove it.
- **Invoice in a diagnostic**: errors are the fixed `invalid_arguments` text; the sentinel sweeps cover the stdin path.
- **Terminal stdin**: with `--yes` and a terminal, the program waits for the line and end of input (Ctrl-D), as any filter does.
