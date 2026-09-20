# mostro-cli — Transport (NIP-44 Direct)

**Status:** nip44 only (protocol v2, signed kind 14).
**Daemon spec:** `MostroP2P/mostro` → `docs/TRANSPORT_V2_SPEC.md`
**Issue:** [#626 — Messaging Transport Abstraction Layer](https://github.com/MostroP2P/mostro/issues/626)
**Core:** `transport` module in **mostro-core** (≥ 0.13; CLI pins 0.14.2)

This is the client-side counterpart to the daemon's transport spec. The CLI
speaks protocol **v2**: signed kind-`14` events with NIP-44 encrypted content.
A v1 node is an error at startup.

## 1. Why

The daemon advertises its wire transport on the kind-`38385` instance-info
event via a `protocol_version` tag (`"2"` = nip44). The CLI must:

- send protocol messages as kind 14, and
- subscribe to / unwrap the same event kind, authored by Mostro.

mostro-core provides the wrap/unwrap entry points; the CLI wires them.

## 2. Wire format

| | v2 (`nip44`) |
|---|---|
| event kind | `14` (signed, NIP-44 content) |
| outer author | **the trade key** (signature is load-bearing) |
| inner payload | 3-tuple `(Message, Option<sig>, identity-proof?)` |
| `Message.version` | 2 |
| expiration | NIP-40 `expiration` tag |

The v2 identity proof lives **inside** the NIP-44 ciphertext (never at the
event level), bound to the authoring trade key. mostro-core handles the tuple,
the proof, and its verification.

> **Note — kind 14 is overloaded.** The CLI also uses kind 14 for protocol#52
> peer/solver chat (`dmtouser` / `getdmuser` / `disputechat`). Protocol
> messages to Mostro are *also* kind 14 but use mostro-core's layout (produced
> by `wrap_message_with`) and are authored by / addressed to Mostro. The two
> are disambiguated on receive by author + `p` tag and by which conversation
> key decrypts (a non-matching event yields `Ok(None)` from `unwrap_incoming`).

## 3. mostro-core APIs the client uses

All re-exported from `mostro_core::prelude`:

- `Transport` — the CLI always uses `Nip44Direct`; `event_kind() -> Kind` (`14`),
  `protocol_version() -> u8` (`2`), `FromStr`/`Display` (`"nip44"`).
- `wrap_message_with(transport, message, identity_keys, trade_keys, receiver, opts) -> Event`
  — send-side wrap for Mostro-protocol messages.
- `unwrap_incoming(event, receiver_keys) -> Option<UnwrappedMessage>`
  — receive-side unwrap; returns `Ok(None)` for "not addressed to me"
  (decrypt miss).

Peer/solver chat uses `mostro_core::chat::{derive_chat_keys, wrap_chat_message,
unwrap_chat_message}` instead of this dispatcher.

## 4. Client wiring

- **Config:** `--transport nip44` (`-t`) sets `TRANSPORT`. Absent/empty ⇒
  nip44. `parse_transport_env()` rejects any other value. This mirrors how
  `POW` / `SECRET` are read from the environment.
- **Send:** the Mostro-protocol path of `send_dm` / `send_plain_text_dm` goes
  through `publish_wrapped` → `wrap_message_with(Transport::Nip44Direct, …)`.
  Peer chat (`dmtouser`, `disputechat`, `sendadmindmattach`) uses
  `wrap_chat_message`.
- **Receive:** `wait_for_dm` subscribes on kind 14 and pins
  `author = mostro_pubkey` so the Mostro reply is never confused with peer
  chat on the same kind.
- **Unwrap:** `parse_dm_events` with `mostro_protocol: true` decodes via
  `unwrap_incoming`; the peer-chat listing path uses `unwrap_chat_message`.
- **Listing / follow-up fetches:** `create_filter` for the `DirectMessages*`
  kinds uses kind 14 and pins `author = mostro_pubkey`. This covers both
  `getdm` and the range-order child-order follow-up after `release`.

## 5. Capability auto-detection

`events::fetch_protocol_version_with` reads the node's `protocol_version` tag
from its kind-38385 info event. `init_context` → `resolve_transport` runs it
once at startup when `--transport` / `TRANSPORT` is unset:

- `2` / absent / unreachable → `TRANSPORT=nip44`
- `1` → error (v1 node)
- An explicit `--transport` is authoritative and skips the probe.

`resolve_transport` logs the active transport and how it was chosen
(`explicit` / `auto-detected protocol v2` / default fallback) at `info`
(shown with `-v`).

## 6. Testing notes

- The daemon under test should have `transport = "nip44"` in `settings.toml`.
- Run the CLI with `--transport nip44` (or omit it: auto-detect from the
  `protocol_version` info tag).
- The daemon's first-contact PoW lane (`pow_first_contact`) is testable by
  combining `--transport nip44` with `--pow <bits>` on the CLI.
- Against a `transport = "nip44"` daemon, a full
  `new-order → take → add-invoice → fiat-sent → release` round-trip should
  succeed. Choosing nip44 is what arms the daemon's anti-spam gates (they
  key off event kind 14, not `Message.version`).
