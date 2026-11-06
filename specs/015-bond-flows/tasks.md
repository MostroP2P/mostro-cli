# Tasks: Bond Flows

**Mode**: lean, as [spec 011](../011-order-mine/tasks.md) (same gates and evidence entry).

- [x] **1. Scripted node.** `step_daemon` follows the source at `waiting-maker-bond` and for a taker's back-out at `waiting-taker-bond`; mail
  constructors for the bond notices.
- [x] **2. Tests.** `tests/bond_flows_process.rs`: both cancels through the binary from a bonded take and a bonded order; the four bond messages
  through `message list --order`.
- [x] **3. Docs.** Command reference and requirement evidence.
