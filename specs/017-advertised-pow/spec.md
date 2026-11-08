# Feature Specification: Mine The Proof Of Work The Node Advertises

**Feature**: `017-advertised-pow`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-07

**Status**: Implemented and verified on Linux against the scripted node, and probed on a local regtest node (see `tests/TEST_SUMMARY.md`).
Progress is tracked only in [tasks.md](tasks.md)

**Input**: mostro-mcp drives `mostro-cli` 0.17.0 with `--output json` and passes `POW` from its environment (unset: 0). The public node
`82fa8cb9…` advertises `pow` 6 and `pow_first_contact` 6 in its kind-38385 info event, so every `order new` failed with `pow_required`,
`publication: not_attempted`, and a message that did not say 6. An agent hosting the MCP server cannot edit its host's environment and
reconnect mid-session. The CLI already read the difficulty from the same probe that feeds the protocol check; it only refused instead of
mining it. `docs/pow_error_handling.md` §5.2 had rejected auto-mining for an interactive terminal; this feature revisits it.

## Requirements

- **FR-001**: Every command that sends one request through `msg::exchange` (`order new`, `order take-sell`, `order take-buy`,
  `order add-invoice`, `bond add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order info`, `dispute open`, `order rate`)
  mines `max(configured, required)`, where `configured` is `--pow` / `POW` (default 0) and `required` is what the node advertises for the
  request's lane, read by the node-info probe the exchange already runs. The event is mined after that probe, not before it.
- **FR-002**: The lane follows mostrod's two-lane gate. Known lane (the signer is the trade key of an order the node holds open: `order
  add-invoice`, `bond add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `dispute open`): `required = pow`. First-contact
  lane (a key the node may not know: the fresh trade key of `order new` and the takes, the identity key of `order info`, the trade key of a
  finished order for `order rate`): `required = max(pow, pow_first_contact)`, absent tags left out, as mostrod advertises.
- **FR-003**: `--pow` / `POW` is a floor: a value above the advertisement is mined as given. A missing, late or malformed info event, or one
  without the lane's tags, mines the configured value only, as before.
- **FR-004**: The program raises the work on its own up to 20 bits (`AUTO_POW_CEILING`, about a million hashes: a mean
  of 1.6 s and a worst of five of 5.2 s on a Ryzen 7 4800H, release build). Above that, when the configured value is
  lower, it publishes nothing: `pow_required` (`error`, `publication: not_attempted`). A configured value at or above
  the requirement is mined whatever its size: the user asked for it.
- **FR-005**: A `pow_required` error carries `error.required` (the difficulty the node advertises, in bits) and `error.configured` (the
  `--pow` / `POW` floor). On the timeout path (`uncertain`), `required` is the highest advertised difficulty above the work sent, from the
  probes before and after the wait. Both are integers 0-255, present on every `pow_required` and on no other code; the generated schemas of
  the commands that can emit `pow_required` say so. The human error line names both numbers and the flag to set.
- **FR-006**: The probe is the one already run per exchange; no extra fetch. Each command sends one request, so nothing is cached across
  processes.
- **FR-007**: The legacy commands keep their behaviour (opt-in `POW`, `PowRequirementUnmet` hints).

## Success Criteria

- **SC-001**: Against a scripted node advertising `pow` 6 and `pow_first_contact` 6, `order new --output json` with `POW` unset is `ok`,
  and the request the node received carries at least 6 bits.
- **SC-002**: A first-contact request mines `pow_first_contact` when it is above `pow`; a known-lane request mines `pow`.
- **SC-003**: `--pow 10` against a node asking for 6 mines at least 10 bits.
- **SC-004**: No info event: the configured value only, as before.
- **SC-005**: A node asking for 21 bits with `POW` unset: `pow_required`, `required` 21, `configured` 0, nothing published; the human line
  names 21 and 0. `--pow 21` sends.
- **SC-006**: The schema fixtures of the commands that can emit `pow_required` describe both fields; the pilot fixtures do not change.
- **SC-007**: A regtest mostrod with `pow` > 0 accepts `order new` with `POW` unset.
