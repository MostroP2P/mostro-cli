# Implementation Plan: Mine The Proof Of Work The Node Advertises

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-07 | **Status**: Implemented with Linux evidence against the scripted node and
a local regtest probe.

- `msg/node_info.rs`: `Lane` (`Known`, `FirstContact`), `NodeInfo::required(lane)` (`pow`, or `max(pow, pow_first_contact)`
  over the tags present), `NodeInfo::unmet(sent)` (the highest advertisement above the work sent, either lane) and the pure
  `work_to_mine(configured, required) -> Result<u8, PowShortfall>` with `AUTO_POW_CEILING` (20). `work_short` and
  `first_contact_work_short` are gone.
- `msg/exchange.rs`: `Outbound.lane`; `pow` documented as the floor. `exchange` no longer wraps before connecting:
  `prepare` probes, checks the protocol, decides the work and wraps (`send_msg::wrap(outbound, pow)`), then `run` arms,
  subscribes and publishes as before. A shortfall above the ceiling is `pow_required` before publishing; on a timeout the
  late probe and the early one give `required` against the work actually mined. `ExchangeFailure::work()` carries the
  numbers.
- `outcome/codes.rs`: `PowShortfall` and the `required` / `configured` fields of `ErrorInfo`, serialized only when set and
  set only on `pow_required` (`with_work`), skipped from the derived schema like `reason`. `Envelope::with_work` /
  `Envelope::work`.
- `outcome/schema.rs`: `add_work` adds both properties (integers 0-255) to the `error` of the failure and uncertain branches
  whose codes include `pow_required`, with an `anyOf` that requires both on that code and forbids them on the others. The
  pilot documents do not change.
- Commands: each `Outbound` names its lane (`new`, `take`, `info`, `rate`: first contact; `invoice`, `step`, `dispute`:
  known; `HeldRequest.lane` for the two held commands); every failure-to-envelope site chains `.with_work(failure.work())`.
- `cli/migrated.rs::error_line`: the one human error line (reason or work numbers), used by `print_info` and the write
  commands' `print_human`. `--pow` gets a help text.
- No cross-process cache: each command runs one exchange, and the exchange already holds the probe it needs.
