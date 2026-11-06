# Implementation Plan: Bond Flows

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Verified; no production change was needed (the four tests passed on the unchanged code). The scripted node was extended.

Tests only, unless one fails: `step_daemon` learns `waiting-maker-bond` and the taker's back-out at `waiting-taker-bond` (the order returns to
`pending`); `tests/bond_flows_process.rs` runs a bonded take and a bonded order through the binary, moves the scripted state to the bond
status, and cancels; the mailbox test publishes the four bond messages and reads them with `message list --order`. Mail constructors for
`bond-invoice-accepted`, `bond-payout-completed` and `bond-slashed` join `fake_mostro.rs`.
