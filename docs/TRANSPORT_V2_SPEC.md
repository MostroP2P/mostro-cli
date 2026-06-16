# mostro-cli — Transport v2 (NIP-44 Direct) client support

**Status:** Phases 1–2 implemented · Phase 3 pending
**Daemon spec:** `MostroP2P/mostro` → `docs/TRANSPORT_V2_SPEC.md`
**Issue:** [#626 — Messaging Transport Abstraction Layer](https://github.com/MostroP2P/mostro/issues/626)
**Core:** `transport` module shipped in **mostro-core 0.13.0**

This document is the client-side counterpart to the daemon's transport-v2
spec. It drives the work of teaching `mostro-cli` to speak protocol **v2**
(signed kind-`14` events with NIP-44 encrypted content) in addition to
protocol **v1** (NIP-59 gift wraps, kind `1059`), so the CLI can trade
against a node running either wire transport — and in particular so we can
exercise the daemon's Phase 2 anti-spam gates (which only engage on the v2
transport).

## 1. Why

The daemon now speaks one of two wire transports per node, selected by its
`[mostro] transport` setting and advertised on the kind-`38385` instance-info
event via a `protocol_versions` tag (`"1"` or `"2"`). A v1-only client cannot
talk to a `transport = "nip44"` node at all (it never sees kind-14 traffic and
its gift wraps are ignored). To test and use v2 nodes, the CLI must:

- send protocol messages through the node's transport, and
- subscribe to / unwrap the matching event kind.

mostro-core 0.13.0 provides everything needed; the CLI work is wiring.

## 2. Wire format recap

| | v1 (`gift-wrap`) | v2 (`nip44`) |
|---|---|---|
| event kind | `1059` (GiftWrap) | `14` (signed, NIP-44 content) |
| outer author | throwaway ephemeral key | **the trade key** (signature is load-bearing) |
| inner payload | 2-tuple `(Message, Option<sig>)` | 3-tuple `(Message, Option<sig>, identity-proof?)` |
| `Message.version` | 2 (since core 0.13) | 2 |
| expiration | none | NIP-40 `expiration` tag |

The v2 identity proof lives **inside** the NIP-44 ciphertext (never at the
event level), bound to the authoring trade key — exactly as private as v1's
seal-carried identity. mostro-core handles the tuple, the proof, and its
verification; the CLI only chooses which wrap/unwrap entry point to call.

> **Note — kind 14 is overloaded.** The CLI already uses kind 14 for NIP-17
> peer-to-peer chat (`SendDm` / `dm-to-user`). Protocol-v2 Mostro messages are
> *also* kind 14 but use mostro-core's `wrap_message_nip44` layout and are
> authored by / addressed to Mostro. The two are disambiguated on receive by
> author + `p` tag and by which conversation key decrypts (a non-matching
> event yields `Ok(None)` from `unwrap_incoming`). Peer chat is out of scope
> for this effort and stays as-is.

## 3. mostro-core 0.13.0 APIs the client uses

All re-exported from `mostro_core::prelude`:

- `Transport` — `enum { GiftWrap, Nip44Direct }`; `event_kind() -> Kind`
  (`1059` / `14`), `protocol_version() -> u8` (`1` / `2`), `FromStr`/`Display`
  (`"gift-wrap"` / `"nip44"`), `Default = GiftWrap`.
- `wrap_message_with(transport, message, identity_keys, trade_keys, receiver, opts) -> Event`
  — send-side dispatcher; routes to gift-wrap or kind-14 wrap.
- `unwrap_incoming(event, receiver_keys) -> Option<UnwrappedMessage>`
  — receive-side dispatcher; routes on `event.kind`, returns `Ok(None)` for
  "not addressed to me" (decrypt miss), same as the existing `unwrap_message`.
- `wrap_message` / `unwrap_message` (the v1 pair) keep their 0.11 signatures —
  no change.

`WrapOptions`, `UnwrappedMessage`, `validate_response`, `Message`, and
`nip59::RANGE_RANDOM_TIMESTAMP_TWEAK` are unchanged from 0.11.3.

## 4. Phases

### Phase 1 — Adopt mostro-core 0.13.0 (foundation) — IMPLEMENTED

Pure dependency bump; **no transport behaviour change** (the CLI still speaks
gift-wrap). De-risks the major-version jump on its own.

- `Cargo.toml`: `mostro-core` `0.11.3` → `0.13.0`.
- The only source break: a non-exhaustive `match` on `order::Status` in
  `src/parser/dms.rs` gains the new `Status::WaitingMakerBond` arm (the bond
  feature's maker-side status; rendered like `WaitingTakerBond`). The new
  `Action`/`Payload` variants (bond + cashu) and `Order`'s new optional Cashu
  fields do not break any call site (existing matches already carry catch-alls;
  `Order` is only deserialized, and the new fields are `Option`).

Effect: messages now carry `Message.version = 2` (core `PROTOCOL_VER`), still
inside gift wraps. The daemon dispatches on event **kind**, not the version
field, and `verify()` validates action↔payload shape (not version), so a 0.13
CLI interoperates with a 0.13 daemon over gift-wrap unchanged. (Talking to a
pre-0.13, version-1 daemon is out of scope — that is the v1-deprecation
timeline's concern.)

Acceptance: `cargo build`, `cargo test`, `cargo clippy --all-targets
--all-features`, `cargo fmt --check` all clean; behaviour identical to before
against a gift-wrap node.

### Phase 2 — Transport selection (v2 capability) — IMPLEMENTED

Teaches the CLI to send and receive on either transport, selected explicitly.

- **Config:** a `--transport <gift-wrap|nip44>` flag (`-t`) that sets a
  `TRANSPORT` env var, resolved via `messaging::parse_transport_env()` into
  `Transport` (default `gift-wrap` — wire-identical to today). This mirrors how
  `POW` / `SECRET` are already read from the environment rather than threaded
  through every call site, so `send_dm`'s signature (and its ~14 callers) is
  untouched. Mirrors the daemon's `[mostro] transport` knob.
- **Send:** the Mostro-protocol path of `send_dm` / `send_plain_text_dm` goes
  through a new `publish_wrapped` → `wrap_message_with(transport, …)`,
  replacing the hard-wired `wrap_message`. The NIP-17 peer-chat path
  (`to_user`) is untouched.
- **Receive:** `wait_for_dm` subscribes on `transport.event_kind()` (and its
  notification loop matches that kind). For v2 it additionally pins
  `author = mostro_pubkey` so the Mostro reply is never confused with NIP-17
  peer chat on the same kind 14.
- **Unwrap:** `parse_dm_events` gains a `mostro_protocol: bool`; when `true` it
  decodes via `unwrap_incoming` (dispatches on kind: 1059 / 14), when `false`
  it keeps the NIP-17 peer-chat path. All Mostro-reply / Mostro→user-DM call
  sites pass `true`; the one peer-chat listing call passes `false`.

Tests: `Transport::from_str` → `event_kind` mapping, and a
`wrap_message_with(Nip44Direct) → unwrap_incoming` roundtrip (kind 14, author =
trade key, message round-trips). Full suite green; clippy + fmt clean.

Known gap (deferred to Phase 3): the `get-dm` **historical listing** filter
(`create_filter` for the `DirectMessages*` kinds) still hard-codes gift wrap,
so listing past Mostro DMs on a v2 node returns nothing. The interactive
request/response path (the one that exercises the daemon's anti-spam gate) is
fully v2.

Acceptance: against a `transport = "nip44"` daemon, run the CLI with
`--transport nip44` and a full `new-order → take → add-invoice → fiat-sent →
release` round-trips; against a gift-wrap daemon (default), behaviour is
unchanged. This is the phase that lets us test the daemon's Phase 2 anti-spam
gates.

### Phase 3 — Capability auto-detection + docs/UX — PENDING

- Make the `get-dm` historical-listing filter transport-aware
  (`create_filter` for the `DirectMessages*` kinds: kind 14 + `author =
  mostro_pubkey` on v2), closing the Phase 2 known gap.
- Read the node's `protocol_versions` tag from its kind-`38385` info event
  (same fetch path as the existing `pow` probe) and, when `--transport` is not
  given, auto-select the matching transport — warning on a mismatch
  ("this node speaks v2; re-run with --transport nip44") instead of silently
  timing out.
- Surface the active transport in verbose output.
- Update `docs/architecture.md`, `docs/commands.md`, and the README.

## 5. Testing notes

- The daemon under test (`MostroP2P/mostro` PR #780) defaults to
  `transport = "gift-wrap"`; set `transport = "nip44"` in its `settings.toml`
  to exercise v2 + the anti-spam gate.
- After Phase 2, run the CLI with `--transport nip44` (or `TRANSPORT=nip44`)
  against that node.
- The daemon's first-contact PoW lane (`pow_first_contact`) is testable by
  combining `--transport nip44` with `--pow <bits>` on the CLI.
