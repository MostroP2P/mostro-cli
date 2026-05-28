# Spec — Surface explicit PoW rejection error

Tracking issue: [MostroP2P/mostro-cli#172](https://github.com/MostroP2P/mostro-cli/issues/172)

## 1. Problem

When a user sends an event from `mostro-cli` to a `mostrod` instance that
**requires NIP‑13 Proof‑of‑Work** without providing the required PoW, the
daemon silently drops the event (logs `Not POW verified event!` on its side)
and never replies. The CLI eventually times out and surfaces a generic
timeout error to the user:

```text
deadline has elapsed
```

(or, in current `main`, the slightly less awful but still ambiguous
`Timeout waiting for DM or gift wrap event`).

This is misleading. From the user perspective the message can mean almost
anything: the relay is slow, mostrod is down, the network is broken, the CLI
is buggy, or the command was wrong. The real cause — PoW not satisfied — is
hidden.

## 2. Root cause

The flow that breaks today:

1. `mostro-cli` mines PoW on the outer GiftWrap based on the `POW` env var
   (default `0`) — see `src/util/messaging.rs::parse_pow_env` and the
   `WrapOptions { pow, .. }` plumbing.
2. The wrapped event is published to relays via `client.send_event(...)`.
3. `wait_for_dm` opens a subscription on the trade key and waits for
   `FETCH_EVENTS_TIMEOUT` (15 s) for an inbound GiftWrap.
4. If `mostrod` requires PoW above what the client provided, mostrod
   silently drops the event in
   [`mostro/src/app.rs`](https://github.com/MostroP2P/mostro) — the relay
   accepted it (so there's no NIP‑01 `OK false` from the relay either) and
   no reply is ever produced.
5. `tokio::time::timeout` elapses → `wait_for_dm` returns `WaitForDmTimeout`
   → the user sees the generic timeout message.

The CLI has **no way to distinguish** "daemon silently dropped my event for
PoW reasons" from "transient relay or network issue".

## 3. Signal we can use

`mostrod` already publishes its required PoW in its **kind‑38385 info event**
under the tag `["pow", "<difficulty>"]`. See
[`mostro/src/nip33.rs`](https://github.com/MostroP2P/mostro) (`new_info_event`)
where the daemon writes:

```rust
Tag::custom(
    TagKind::Custom(Cow::Borrowed("pow")),
    vec![mostro_settings.pow.to_string()],
),
```

Reading that tag on the CLI side lets us answer the only question we care
about: *did this Mostro instance reject our event because we didn't provide
enough PoW?*

We already fetch kind‑38385 elsewhere in the CLI — see
`src/util/events.rs::fetch_bond_claim_window_days` — so this is a small
extension of an existing pattern.

## 4. Design

### 4.1 Error variant

Add a typed error so callers (and tests) can match the PoW failure mode
distinctly from a generic timeout:

```rust
#[derive(Debug)]
pub struct PowRequirementUnmet {
    pub required: u8,
    pub configured: u8,
}
```

The user-facing `Display` will be explicit:

```text
This Mostro instance requires NIP-13 proof of work of N bits, but the
client sent the event with M bits. Re-run with `--pow N` or set
`POW=N` and try again.
```

The error lives next to `WaitForDmTimeout` in `src/util/messaging.rs` (same
module, same idiom: small zero/struct error wrapped via `anyhow`).

### 4.2 Helper — fetch required PoW from kind‑38385

Add a helper in `src/util/events.rs`:

```rust
/// Best-effort: fetch the Mostro instance's kind-38385 info event and read
/// the `pow` tag. Returns `None` when no info event is available or the tag
/// is missing/unparseable. Mirrors `fetch_bond_claim_window_days`.
pub async fn fetch_required_pow(ctx: &crate::cli::Context) -> Option<u8>;
```

Implementation details:

- Author filter on `ctx.mostro_pubkey`, kind `NOSTR_INFO_EVENT_KIND`.
- Pick the **newest** revision by `created_at` (replaceable but lagging
  relays can still serve old copies; same caveat as the bond-window helper).
- Scan tags for `["pow", "<value>"]`, parse as `u8`.
- Any error (fetch failure, missing tag, unparseable value) → `None`.

Returning `None` is the "I don't know" signal; the caller treats it as
*"can't blame PoW"* and falls back to the generic timeout error. This keeps
the helper non-fatal: older daemons that don't publish the tag, or unreachable
relays, never break flows that worked before.

### 4.3 Where to plug it in — `wait_for_dm`

`wait_for_dm` is the single chokepoint that every request/reply flow goes
through (`add_invoice`, `take_order`, `take_dispute`, `send_msg`, `new_order`,
`rate_user`, `orders_info`, `restore`, `last_trade_index`, `add_bond_invoice`).
Centralizing the fix here covers every command in one place.

Concurrent probe (chosen — see Alternatives below):

```rust
// Kick off the PoW probe alongside the DM wait so its answer is in hand
// the moment the wait times out. The probe is cheap to start and cheap to
// cancel via JoinHandle::abort() on the happy path.
let pow_probe = tokio::spawn(fetch_required_pow_with(
    ctx.client.clone(),
    ctx.mostro_pubkey,
));

let waited = tokio::time::timeout(FETCH_EVENTS_TIMEOUT, /* notification loop */).await;

let event = match waited {
    Ok(inner) => {
        pow_probe.abort();
        inner?
    }
    Err(_elapsed) => {
        // Probe has been running for FETCH_EVENTS_TIMEOUT alongside the
        // wait; it should already be done. POW_PROBE_TIMEOUT is a safety
        // net for pathological relays — if the answer isn't in by then,
        // fall through to the generic timeout error.
        let probe_result = tokio::time::timeout(POW_PROBE_TIMEOUT, pow_probe).await;
        if let Ok(Ok(Some(required))) = probe_result {
            let configured = parse_pow_env().unwrap_or(0);
            if required > configured {
                return Err(PowRequirementUnmet { required, configured }.into());
            }
        }
        return Err(WaitForDmTimeout.into());
    }
};
```

The probe lives in `events::fetch_required_pow_with(client, mostro_pubkey)`
— an owned-args sibling of `fetch_required_pow(ctx)`, used so the spawned
future is `'static`. The 3 s `POW_PROBE_TIMEOUT` is now a safety net rather
than the typical wait: in the common timeout case the probe is already
resolved when we look at it, so the user-visible wait stays at
`FETCH_EVENTS_TIMEOUT` (15 s) plus ~0 s, instead of doubling to 30 s as the
naive sequential version would.

Add an `&Context` parameter? Look at the signature today —
`wait_for_dm(ctx, order_trade_keys, sent_message)` — `ctx` is already
passed. We just need to grant the helper access to `ctx.client` and
`ctx.mostro_pubkey` (already does).

### 4.4 `add_bond_invoice` interplay

`add_bond_invoice` treats `WaitForDmTimeout` as the happy path (Mostro pays
the invoice without acking over Nostr). The new variant must **not** be
caught by that branch — a PoW failure on `add-bond-invoice` is *not* a
successful submission. Concretely:

- The existing `Err(e) if e.downcast_ref::<WaitForDmTimeout>().is_some()`
  arm continues to match only `WaitForDmTimeout`.
- A `PowRequirementUnmet` falls through to the generic `Err(e) => return
  Err(e)` arm and bubbles up to the user, which is the desired behavior.

No code change needed in `add_bond_invoice.rs` — the existing pattern match
already handles the right split. We assert this with a regression note.

## 5. Alternatives considered

### 5.1 Preflight check (fail before sending)

Run `fetch_required_pow` *before* `send_dm`. Advantages: fail fast (no
15‑second wait). Disadvantages:

- Adds a relay roundtrip to the happy path for every command.
- Doubles "you didn't set PoW" errors for daemons that publish the tag but
  also accept PoW=0 (no real check on that side today).
- Couples every command to the info event being reachable.

→ Rejected. Postflight has the right cost model: zero overhead when things
work, and *only* pays for the info-event fetch in the failure path where the
user is already waiting anyway.

### 5.2 Auto-mine the required PoW

Bump `WrapOptions.pow` to `max(configured, required)` instead of erroring.
Tempting, but high PoW targets (e.g. 28+ bits) can take minutes to mine on
a laptop, and the user wouldn't see *why* the CLI is suddenly chewing CPU.
Better to fail explicitly with an actionable hint and let the user opt in
by setting `POW` themselves.

→ Rejected. Defer to explicit user opt-in via `--pow` / `POW` env.

### 5.3 Read NIP‑01 `OK false` from the relay

Won't help: mostrod drops the event *after* the relay accepts it, so the
relay returns `OK true`. There is no rejection signal on the publish path.

→ Rejected.

## 6. Acceptance criteria

(Reproducing the issue's checklist, with concrete bindings.)

- Sending an event from `mostro-cli` to a PoW‑enabled `mostrod` without
  satisfying PoW must surface an error that mentions PoW. Specifically the
  `PowRequirementUnmet { required, configured }` variant rendered as:
  `"This Mostro instance requires NIP-13 proof of work of N bits, but the
  client sent the event with M bits. Re-run with --pow N or set POW=N and
  try again."`
- Genuine timeouts (daemon reachable, no PoW requirement, just slow / no
  reply) keep returning `WaitForDmTimeout` — its existing message stays.
- `add_bond_invoice`'s "timeout is the happy path" arm only matches
  `WaitForDmTimeout`; a `PowRequirementUnmet` propagates and is shown to
  the user.
- Older daemons that don't publish `["pow", ...]` in kind‑38385 behave
  exactly as before (helper returns `None` → generic timeout).

## 7. Test plan

Unit tests in `src/util/messaging.rs` and `src/util/events.rs`:

- `pow_requirement_unmet_display_mentions_required_and_configured` —
  formatting check on the error message.
- `fetch_required_pow_returns_none_when_tag_missing` — synthesize a
  kind-38385 event with no `pow` tag, assert `None`.
- `fetch_required_pow_picks_newest_revision` — two events, newer with a
  different pow value; helper returns the newer value. Mirrors the existing
  `fetch_bond_claim_window_days` pattern.
- `fetch_required_pow_parses_pow_tag` — single event with `["pow", "12"]`,
  assert `Some(12)`.

A manual end-to-end check (out of the unit-test scope) is to run against a
PoW‑enabled local `mostrod`:

```sh
POW=0 mostro-cli neworder ...
# expected: PowRequirementUnmet error, not "deadline has elapsed"
POW=<required> mostro-cli neworder ...
# expected: normal flow, no error
```

## 8. Out of scope

- Auto-mining the missing PoW (see 5.2).
- Removing or restructuring the `POW` env var / `--pow` flag.
- Surfacing the PoW requirement during `list-orders` / informational
  flows. The fix is scoped to the request/reply path where the missing
  PoW silently kills the flow.
