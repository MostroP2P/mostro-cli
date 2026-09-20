## `senddm` flow (`src/cli/send_dm.rs`)

This document explains the full flow for the `senddm` command, from CLI invocation to the Nostr event sent on the relays, including which keys and protocol pieces are involved.

### 1. CLI entrypoint

- **Command**: `senddm`
- **Defined in**: `src/cli.rs` (`Commands::SendDm`)
- **Documented in**: `docs/commands.md`
- **Args**:
  - `--pubkey <NPUB/HEX>`: Recipient pubkey.
  - `--order-id <UUID>`: Order identifier (used to select the correct trade key).
  - `--message <STRING>...`: One or more message parts, joined with spaces.

The CLI argument parser constructs a `Commands::SendDm { pubkey, order_id, message }` variant and then calls:

- `Commands::run(&self, ctx: &Context)` in `src/cli.rs`, which dispatches to:
  - `execute_send_dm(PublicKey::from_str(pubkey)?, ctx, order_id, &msg)` in `src/cli/send_dm.rs`.

The shared `Context` (`src/cli.rs`) contains:

- `identity_keys: Keys`: long‑term i0 identity keys for the user.
- `trade_keys: Keys`: current ephemeral trade keys (derived from identity via BIP32/NIP‑06).
- `client: Client`: connected Nostr client (relays from `RELAYS` env var).
- `pool: SqlitePool`: SQLite connection (orders and users).
- `mostro_pubkey: PublicKey`: Mostro service pubkey.
- `context_keys: Option<Keys>`: admin keys when running admin commands.

### 2. High‑level handler (`execute_send_dm`)

File: `src/cli/send_dm.rs`

```rust
pub async fn execute_send_dm(
    receiver: PublicKey,
    ctx: &Context,
    order_id: &Uuid,
    message: &str,
) -> Result<()> {
    // 1) Print a summary table (order id, recipient, message)
    // 2) Build a Mostro-core Message (Action::SendDm, Payload::TextMessage)
    // 3) Resolve the trade keys for this order from the DB
    // 4) Delegate to util::send_dm to construct and send the Nostr event
}
```

Step‑by‑step:

1. **UI / logging**:
   - Builds a table with:
     - `Order ID`
     - `Recipient` (receiver pubkey)
     - `Message`
   - Prints it to the terminal as human‑friendly confirmation.

2. **Mostro protocol payload**:
   - Constructs a Mostro‑core `Message`:

     ```rust
     let message = Message::new_dm(
         None,
         None,
         Action::SendDm,
         Some(Payload::TextMessage(message.to_string())),
     )
     .as_json()
     .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;
     ```

   - Semantics:
     - `Action::SendDm`: high‑level Mostro action.
     - `Payload::TextMessage(...)`: plain text content to deliver.
     - `request_id` and some other fields are `None` here, since this is a simple DM.
   - The result is a **JSON string** (Mostro protocol message) that will be used as the encrypted DM payload on Nostr.

3. **Resolve trade keys for this order**:

   ```rust
   let trade_keys =
       if let Ok(order_to_vote) = Order::get_by_id(&ctx.pool, &order_id.to_string()).await {
           match order_to_vote.trade_keys.as_ref() {
               Some(trade_keys) => Keys::parse(trade_keys)?,
               None => {
                   anyhow::bail!("No trade_keys found for this order");
               }
           }
       } else {
           return Err(anyhow::anyhow!("order {} not found", order_id));
       };
   ```

   - Orders in the DB store a serialized `trade_keys` field (per‑order ephemeral keys).
   - These keys are:
     - Derived from the user’s mnemonic (see `src/db.rs` and NIP‑06 support).
     - Used as the **Nostr keypair for this trade**.
     - Used for DM encryption and as the sender identity on Nostr.

4. **Delegate to `util::send_dm`**:

   ```rust
   send_dm(
       &ctx.client,
       &ctx.identity_keys,
       &trade_keys,
       &receiver,
       message,
       None,
       false,
   )
   .await?;
   ```

   - `client`: connected Nostr client (relays from `RELAYS`).
   - `identity_keys`: long-term identity; signs the inner identity proof.
   - `trade_keys`: the per-order trade keys that author the kind-14 event.
   - `receiver`: target Nostr pubkey (user or service).
   - `payload`: the serialized Mostro `Message` JSON built above.
   - `expiration: None`: no extra NIP‑40 expiration tags.
   - `to_user: false`: Mostro-protocol wrap (see below), not NIP-17 peer chat.

### 3. Low‑level DM construction (`util::send_dm`)

File: `src/util/messaging.rs`

```rust
pub async fn send_dm(
    client: &Client,
    identity_keys: &Keys,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    payload: String,
    expiration: Option<Timestamp>,
    to_user: bool,
) -> Result<()> {
    let pow = parse_pow_env()?;

    if to_user {
        let event = create_private_dm_event(trade_keys, receiver_pubkey, payload, pow).await?;
        client.send_event(&event).await?;
        return Ok(());
    }

    let message = Message::from_json(&payload)?;
    let private = parse_secret_env()?;
    let opts = WrapOptions {
        pow,
        expiration,
        signed: !private,
    };

    publish_wrapped(
        client,
        parse_transport_env()?,
        identity_keys,
        trade_keys,
        receiver_pubkey,
        &message,
        opts,
    )
    .await
}
```

Key points:

- **POW**:
  - `POW` env var (default `"0"`) controls proof‑of‑work difficulty for the outer kind-14 event.
- **SECRET**:
  - `SECRET=true` leaves the inner tuple unsigned (full-privacy mode: identity = trade key).
- **`to_user`**:
  - `true` — NIP-17 `PrivateDirectMessage` (kind 14 signed directly by `trade_keys`).
  - `false` — Mostro-protocol message via `wrap_message_with(Transport::Nip44Direct, …)`.

  For `senddm`, `to_user` is **`false`**.

#### 3.1 Mostro-protocol kind-14 DM (default `senddm` mode)

`publish_wrapped` calls `wrap_message_with` with `Transport::Nip44Direct`:

- **Inner content**:
  - Parses the Mostro `Message` from `payload`.
  - When `signed = true` (default): identity proof is bound to the trade key
    inside the NIP-44 ciphertext.
  - When `SECRET=true`: the inner tuple is unsigned.
- **Outer event**:
  - Kind 14, authored by `trade_keys`, NIP-44 encrypted to `receiver_pubkey`.
  - Optional NIP-13 PoW from `POW`.
  - Optional NIP-40 expiration (unused here).
- **Relaying**:
  - `client.send_event(&event).await?` to every relay in `RELAYS`.

### 4. Keys and protocols summary

- **Keys**:
  - `identity_keys` (i0): long‑term user identity (stored in DB).
  - `trade_keys`: per-order ephemeral keys used for:
    - Authoring the kind-14 event.
    - Signing the Mostro inner tuple when not in secret mode.
  - `receiver_pubkey`: DM target (user or service).

- **Protocols**:
  - **Mostro application protocol**:
    - `Message::new_dm` + `Action::SendDm` + `Payload::TextMessage`.
  - **Nostr**:
    - NIP‑13 (optional POW).
    - NIP‑40 (optional expiration tags, not used here).
    - NIP‑44 kind 14 for encapsulating the Mostro message.
