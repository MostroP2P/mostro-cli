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

```12:69:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_dm.rs
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

     ```35:42:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_dm.rs
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

   ```44:53:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_dm.rs
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

   ```56:64:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_dm.rs
   send_dm(
       &ctx.client,
       Some(&trade_keys),
       &trade_keys,
       &receiver,
       message,
       None,
       false,
   )
   .await?;
   ```

   - `client`: connected Nostr client (relays from `RELAYS`).
   - `identity_keys: Some(&trade_keys)`: used for signing when we choose the "signed gift wrap" mode.
   - `trade_keys`: the per‑order trade keys used for DM encryption / gift wrap.
   - `receiver`: target Nostr pubkey (user or service).
   - `payload`: the serialized Mostro `Message` JSON built above.
   - `expiration: None`: no extra NIP‑40 expiration tags.
   - `to_user: false`: this controls which DM mode is used (see below).

### 3. Low‑level DM construction (`util::send_dm`)

File: `src/util/messaging.rs`

```201:253:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/util/messaging.rs
pub async fn send_dm(
    client: &Client,
    identity_keys: Option<&Keys>,
    trade_keys: &Keys,
    receiver_pubkey: &PublicKey,
    payload: String,
    expiration: Option<Timestamp>,
    to_user: bool,
) -> Result<()> {
    let pow: u8 = var("POW")
        .unwrap_or('0'.to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse POW: {}", e))?;
    let private = var("SECRET")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .map_err(|e| anyhow::anyhow!("Failed to parse SECRET: {}", e))?;

    let message_type = determine_message_type(to_user, private);

    let event = match message_type {
        MessageType::PrivateDirectMessage => {
            create_private_dm_event(trade_keys, receiver_pubkey, payload, pow).await?
        }
        MessageType::PrivateGiftWrap => {
            create_gift_wrap_event(
                trade_keys,
                identity_keys,
                receiver_pubkey,
                payload,
                pow,
                expiration,
                false,
            )
            .await?
        }
        MessageType::SignedGiftWrap => {
            create_gift_wrap_event(
                trade_keys,
                identity_keys,
                receiver_pubkey,
                payload,
                pow,
                expiration,
                true,
            )
            .await?
        }
    };

    client.send_event(&event).await?;
    Ok(())
}
```

Key points:

- **POW**:
  - `POW` env var (default `"0"`) controls proof‑of‑work difficulty for the outer Nostr event.
- **SECRET**:
  - `SECRET` env var (`"true"/"false"`) controls whether messages are sent as private DMs vs gift wraps.

- **Message type decision**:

  ```129:134:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/util/messaging.rs
  fn determine_message_type(to_user: bool, private: bool) -> MessageType {
      match (to_user, private) {
          (true, _) => MessageType::PrivateDirectMessage,
          (false, true) => MessageType::PrivateGiftWrap,
          (false, false) => MessageType::SignedGiftWrap,
      }
  }
  ```

  For `senddm`:
  - `to_user` is **`false`**
  - `SECRET` defaults to **`false`**
  - So we use **`MessageType::SignedGiftWrap`**

#### 3.1 Signed gift wrap DM (default `senddm` mode)

For `MessageType::SignedGiftWrap` we use `create_gift_wrap_event(..., signed = true)`:

```162:199:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/util/messaging.rs
async fn create_gift_wrap_event(
    trade_keys: &Keys,
    identity_keys: Option<&Keys>,
    receiver_pubkey: &PublicKey,
    payload: String,
    pow: u8,
    expiration: Option<Timestamp>,
    signed: bool,
) -> Result<nostr_sdk::Event> {
    let message = Message::from_json(&payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize message: {e}"))?;

    let content = if signed {
        let _identity_keys = identity_keys
            .ok_or_else(|| Error::msg("identity_keys required for signed messages"))?;
        let sig = Message::sign(payload, trade_keys);
        serde_json::to_string(&(message, sig))
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    } else {
        let content: (Message, Option<Signature>) = (message, None);
        serde_json::to_string(&content)
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {e}"))?
    };

    let rumor = EventBuilder::text_note(content)
        .pow(pow)
        .build(trade_keys.public_key());

    let tags = create_expiration_tags(expiration);

    let signer_keys = if signed {
        identity_keys.ok_or_else(|| Error::msg("identity_keys required for signed messages"))?
    } else {
        trade_keys
    };

    Ok(EventBuilder::gift_wrap(signer_keys, receiver_pubkey, rumor, tags).await?)
}
```

Protocol behaviour:

- **Inner content**:
  - Parses the Mostro `Message` from `payload`.
  - If `signed = true`:
    - Computes a Mostro‑level signature: `Message::sign(payload, trade_keys)`.
    - Wraps `(message, sig)` into JSON.
  - Builds a text‑note rumor event:
    - `kind`: `TextNote`
    - `content`: JSON `(Message, Signature)` or `(Message, None)`.
    - `pubkey`: `trade_keys.public_key()`
    - Optional POW as configured by `POW`.

- **Outer NIP‑59 Gift Wrap**:
  - `EventBuilder::gift_wrap(...)` wraps the rumor into a **GiftWrap** event (NIP‑59).
  - `signer_keys`:
    - For `senddm` default, `signed = true`, so `signer_keys = identity_keys`, passed as `Some(&trade_keys)`.
    - This means the outer event is also signed by the **trade keys**.
  - `receiver_pubkey`: the `receiver` passed to `execute_send_dm` (user/Mostro/other).
  - `tags`: optional NIP‑40 expiration (unused here).

- **Relaying**:
  - The final event is sent via:

    ```251:252:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/util/messaging.rs
    client.send_event(&event).await?;
    ```

  - `client` is a `nostr_sdk::Client` connected to all relays in `RELAYS`.

### 4. Keys and protocols summary

- **Keys**:
  - `identity_keys` (i0): long‑term user identity (stored in DB).
  - `trade_keys`: per‑order ephemeral keys used for:
    - DM identity on Nostr.
    - Signing Mostro messages (`Message::sign`).
  - `receiver_pubkey`: DM target (user or service).

- **Protocols**:
  - **Mostro application protocol**:
    - `Message::new_dm` + `Action::SendDm` + `Payload::TextMessage`.
  - **Nostr**:
    - NIP‑13 (optional POW).
    - NIP‑40 (optional expiration tags, not used here).
    - NIP‑59 Gift Wrap for encapsulating the Mostro message.


