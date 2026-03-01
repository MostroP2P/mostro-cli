## `sendadmindmattach` flow (`src/cli/send_admin_dm_attach.rs`)

This document explains the full flow for the `sendadmindmattach` command: from CLI invocation, through Blossom upload, to the final encrypted DM sent on Nostr, including protocols and keys involved.

### 1. CLI entrypoint

- **Command**: `sendadmindmattach` *(admin only)*
- **Defined in**: `src/cli.rs` (`Commands::SendAdminDmAttach`)
- **Documented in**: `docs/commands.md`
- **Args**:
  - `--pubkey <NPUB/HEX>`: Admin / solver recipient pubkey.
  - `--order-id <UUID>`: Order ID to pick the correct trade keys.
  - `--file <PATH>`: Path to the file to encrypt and upload.
- **Handler**:
  - `execute_send_admin_dm_attach(PublicKey::from_str(pubkey)?, ctx, order_id, file)` in `src/cli/send_admin_dm_attach.rs`.

The global `Context` (`src/cli.rs`) provides:

- `identity_keys: Keys`: user’s i0 identity keys.
- `trade_keys: Keys`: current per‑trade keys.
- `client: Client`: Nostr client connected to relays from `RELAYS`.
- `pool: SqlitePool`: database for orders and users.
- `context_keys: Option<Keys>`: admin keys when `ADMIN_NSEC` is configured.
- `mostro_pubkey: PublicKey`: Mostro service key (not directly used here).

### 2. Handler: `execute_send_admin_dm_attach`

File: `src/cli/send_admin_dm_attach.rs`

```198:335:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
pub async fn execute_send_admin_dm_attach(
    receiver: PublicKey,
    ctx: &Context,
    order_id: &Uuid,
    file_path: &PathBuf,
) -> Result<()> {
    // 1) Validate file and show summary
    // 2) Load order and per-order trade keys from DB
    // 3) Derive shared key with admin pubkey and encrypt file (ChaCha20-Poly1305)
    // 4) Upload encrypted blob to Blossom with BUD-01 auth
    // 5) Build DM payload JSON referencing Blossom URL + crypto metadata
    // 6) Gift-wrap DM and send over Nostr
}
```

Step‑by‑step:

1. **File validation & UI**:
   - `fs::metadata(file_path)`:
     - Ensures the path is a regular file.
     - Enforces a hard size limit: `MAX_FILE_SIZE_BYTES = 25 MiB`.
   - Reads file data into memory:

     ```220:221:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
     let file_bytes = fs::read(file_path)
         .map_err(|e| anyhow::anyhow!("failed to read file {}: {e}", file_path.display()))?;
     ```

   - Builds and prints a table with:
     - Order ID
     - Trade pubkey
     - Admin pubkey (receiver)
     - File path
     - File size

2. **Resolve per‑order trade keys**:

   ```223:230:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
   let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
       .await
       .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;

   let trade_keys = match order.trade_keys.as_ref() {
       Some(trade_keys) => Keys::parse(trade_keys)?,
       None => anyhow::bail!("No trade_keys found for this order"),
   };
   ```

   - Orders persisted in SQLite store a `trade_keys` string.
   - `Keys::parse` reconstructs the **per‑order ephemeral keypair**.
   - These trade keys are used for:
     - Encrypting the attachment (via ECDH shared secret).
     - Sending/signing the Nostr DM (gift‑wrapped text note).

### 3. ECDH shared key & symmetric encryption

#### 3.1 Derive ECDH shared key

```36:50:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
fn derive_shared_key(trade_keys: &Keys, admin_pubkey: &PublicKey) -> [u8; 32] {
    use bitcoin::secp256k1::ecdh::shared_secret_point;
    use bitcoin::secp256k1::{Parity, PublicKey as SecpPublicKey};

    let sk = trade_keys.secret_key();
    let xonly = admin_pubkey
        .xonly()
        .expect("failed to get x-only public key for admin");
    let secp_pk = SecpPublicKey::from_x_only_public_key(xonly, Parity::Even);
    let mut point_bytes = shared_secret_point(&secp_pk, sk).as_slice().to_vec();
    point_bytes.resize(32, 0);
    point_bytes
        .try_into()
        .expect("shared secret point must be at least 32 bytes")
}
```

- **Inputs**:
  - `trade_keys.secret_key()`: ECDSA secp256k1 secret for the order.
  - `admin_pubkey`: receiver’s Nostr pubkey (converted to x‑only form).
- **Operation**:
  - Computes an ECDH shared secret point using secp256k1.
  - Takes the x‑coordinate bytes, truncates/pads to 32 bytes.
- **Output**:
  - A 32‑byte shared secret `[u8; 32]` used as the **symmetric key** for file encryption.

#### 3.2 Encrypt blob with ChaCha20‑Poly1305

```52:83:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
fn encrypt_blob(shared_key: [u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, String)> {
    let key = Key::from_slice(&shared_key);
    let cipher = ChaCha20Poly1305::new(key);

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    if ciphertext.len() < 16 {
        return Err(anyhow::anyhow!(
            "ciphertext too short, expected at least 16 bytes auth tag"
        ));
    }

    let (encrypted_data, auth_tag) = ciphertext.split_at(plaintext.len());

    let mut blob = Vec::with_capacity(12 + encrypted_data.len() + auth_tag.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(encrypted_data);
    blob.extend_from_slice(auth_tag);

    let nonce_hex = nonce_bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    Ok((blob, nonce_hex))
}
```

Encryption details:

- Algorithm: **ChaCha20‑Poly1305 AEAD**.
- Nonce: random 12‑byte value from `OsRng`.
- Ciphertext layout:
  - First 12 bytes: nonce.
  - Next `plaintext.len()` bytes: encrypted payload.
  - Final 16 bytes: Poly1305 authentication tag.
- Returned values:
  - `blob`: `nonce || encrypted_data || auth_tag` (the bytes sent to Blossom).
  - `nonce_hex`: hex‑encoded nonce, needed later to decrypt the blob.

In the handler:

```263:266:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
let shared_key = derive_shared_key(&trade_keys, &receiver);
let (encrypted_blob, nonce_hex) = encrypt_blob(shared_key, &file_bytes)?;
let encrypted_size = encrypted_blob.len();
let blossom_url = upload_to_blossom(&trade_keys, encrypted_blob).await?;
```

### 4. Blossom upload (BUD‑01/BUD‑02, kind 24242)

The encrypted blob is uploaded to a **Blossom** server. Upload is authenticated using Blossom’s BUD‑01/BUD‑02 spec:

- **Auth event kind**: `24242` (`Kind::BlossomAuth`).
- **Auth tags**:
  - `["t", "upload"]`: verb for upload action.
  - `["expiration", "<future unix ts>"]`: NIP‑40 expiration, must be in the future.
  - `["x", "<sha256 hex>"]`: SHA256 of the encrypted blob.

Implementation:

```85:196:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
async fn upload_to_blossom(trade_keys: &Keys, encrypted_blob: Vec<u8>) -> Result<String> {
    use reqwest::StatusCode;

    let client = reqwest::Client::new();
    let payload_hash = Sha256Hash::hash(&encrypted_blob);
    let payload_hex = payload_hash.to_string();

    // Expiration: 1 hour from now (BUD-01 requires expiration in the future)
    let expiration = Timestamp::from(Timestamp::now().as_u64() + 3600);

    for server in BLOSSOM_SERVERS {
        let url_str = format!("{}/upload", server.trim_end_matches('/'));
        let upload_url = match Url::parse(&url_str) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("Blossom invalid URL {url_str}: {e}");
                continue;
            }
        };

        let normalized_url_str = upload_url.as_str();

        // BUD-01: kind 24242, content human-readable, tags: t=upload, expiration, x=sha256
        let tags = [
            Tag::hashtag("upload"), // ["t", "upload"]
            Tag::expiration(expiration),
            Tag::custom(TagKind::x(), [payload_hex.clone()]),
        ];
        let event = match EventBuilder::new(Kind::BlossomAuth, "Upload Blob")
            .tags(tags)
            .sign_with_keys(trade_keys)
        {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Blossom auth event build failed for {server}: {e}");
                continue;
            }
        };
        let auth_header = format!("Nostr {}", BASE64.encode(event.as_json()));

        // Many Blossom servers reject application/octet-stream. Try application/zip first, then image/png.
        let content_types = ["application/zip", "image/png"];
        let mut last_status = None;
        let mut last_error_body = String::new();

        for content_type in content_types {
            let res = client
                .put(normalized_url_str)
                .header("Content-Type", content_type)
                .header("Authorization", &auth_header)
                .body(encrypted_blob.clone())
                .send()
                .await;

            match res {
                Ok(resp) if resp.status() == StatusCode::OK => {
                    let body = resp
                        .text()
                        .await
                        .map_err(|e| anyhow::anyhow!("failed to read blossom response: {e}"))?;
                    let body = body.trim();
                    // BUD-02: server may return JSON blob descriptor with "url" field
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                        if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
                            return Ok(url.to_string());
                        }
                    }
                    if body.starts_with("blossom://") {
                        return Ok(body.to_string());
                    }
                    last_status = Some(StatusCode::OK);
                    last_error_body = body.to_string();
                    break;
                }
                Ok(resp) => {
                    let status = resp.status();
                    last_status = Some(status);
                    last_error_body = resp.text().await.unwrap_or_default();
                    // If rejected for content type, try next; otherwise report and break
                    if status != StatusCode::UNSUPPORTED_MEDIA_TYPE
                        && status != StatusCode::BAD_REQUEST
                    {
                        break;
                    }
                }
                Err(e) => {
                    last_status = None;
                    last_error_body = e.to_string();
                    break;
                }
            }
        }

        if let Some(status) = last_status {
            let status_text = status.canonical_reason().unwrap_or("Unknown");
            eprintln!(
                "Blossom upload failed on {server} with status {} {}",
                status, status_text
            );
            if !last_error_body.is_empty() && last_error_body.len() < 500 {
                eprintln!("  Response body: {}", last_error_body);
            }
        } else {
            eprintln!("Blossom upload error on {server}: {}", last_error_body);
        }
    }

    Err(anyhow::anyhow!("all Blossom servers failed"))
}
```

Blossom protocol details:

- **Auth event**:
  - `kind = 24242` (`Kind::BlossomAuth`).
  - `content = "Upload Blob"` (human‑readable).
  - `tags`:
    - `["t", "upload"]` – operation verb.
    - `["expiration", "<unix ts>"]` – must be in the future.
    - `["x", "<sha256>"]` – encoded via `TagKind::x()`.
  - Encoded as:
    - `Authorization: Nostr <base64(event_json)>`.

- **Request**:
  - `PUT /upload` with body = `encrypted_blob`.
  - `Content-Type` fallback:
    - Tries `application/zip`, then `image/png` if the server responds with 400/415 for content type.

- **Response handling**:
  - 200 OK:
    - If body is JSON with `"url"`: returns that URL (BUD‑02 Blob Descriptor).
    - Else if body starts with `blossom://`: returns that URI.
  - Non‑200 or no usable response: logs error and tries next server.

The function ultimately returns a **public URL** (`blossom_url`) pointing to the encrypted blob.

### 5. DM payload and shared-key custom wrap to admin

Once the encrypted blob is uploaded and we have `blossom_url`, the DM payload is built as JSON (`type`, `blossom_url`, `nonce`, `mime_type`, sizes, `filename`). This content is then sent using **shared-key custom wrap** (same pattern as `dmtouser`):

- **Shared key**: The same ECDH shared key used for file encryption (trade keys + admin pubkey) is turned into a `Keys` via `Keys::new(SecretKey::from_slice(&shared_key)?)`.
- **Send**: `send_admin_chat_message_via_shared_key(&ctx.client, &trade_keys, &shared_keys, &content)` in `src/util/messaging.rs`:
  - Builds an inner text-note event (sender = trade keys), signs it, encrypts it with NIP-44 to the **shared key’s public key**, and wraps it in a NIP-59 GiftWrap event tagged with that pubkey (`#p`).
  - Both the sender (trade keys) and the admin (who can derive the same shared key) can later fetch and decrypt the event by filtering GiftWrap by the shared key pubkey and using `unwrap_giftwrap_with_shared_key`.

So the attachment metadata is not sent as a plain NIP-59 gift wrap to the admin pubkey; it is sent to the **shared key’s public key**, enabling symmetric decryption for both parties. Relaying is via `ctx.client.send_event(&event)`.

### 6. Keys and protocols summary

- **Keys**:
  - `trade_keys` (per‑order):
    - Used for:
      - ECDH shared secret with admin pubkey (for file encryption and for the shared-key DM).
      - Nostr identity for the inner text note and for signing the outer NIP‑59 wrap.
      - Signing the Blossom auth event (kind 24242).
  - **Shared key** (ECDH from trade_keys + admin pubkey):
    - Same 32-byte secret used for ChaCha20‑Poly1305 file encryption.
    - Wrapped as `Keys` and used as the **recipient** of the DM: the NIP-59 GiftWrap is addressed to the shared key’s public key, and the inner content is NIP-44 encrypted to it, so both sender and admin can derive the key and decrypt.
  - `receiver`:
    - Admin / solver Nostr pubkey; used to derive the shared key and as the human-facing destination (the actual Nostr event recipient is the shared key pubkey).

- **Protocols**:
  - **ChaCha20‑Poly1305**:
    - Symmetric AEAD encryption of the file using ECDH‑derived key.
  - **Blossom (BUD‑01/BUD‑02)**:
    - Auth event: kind 24242, tags `t=upload`, `expiration`, `x=<sha256>`.
    - Upload endpoint: `PUT /upload` with binary body and auth header.
    - Response: JSON Blob Descriptor (with `url`) or `blossom://…` URI.
  - **Mostro DM payload**:
    - JSON with attachment metadata:
      - `type`, `blossom_url`, `nonce`, `mime_type`, sizes, `filename`.
  - **Nostr**:
    - NIP‑13: optional POW on the inner text note.
    - NIP‑44: encryption of the inner event to the shared key’s public key.
    - NIP‑59: GiftWrap envelope addressed to the **shared key pubkey** (not directly to the admin), so both parties that know the ECDH secret can fetch and decrypt.

End‑to‑end, `sendadmindmattach`:

1. Derives a shared ECDH key between trade keys and admin pubkey.
2. Encrypts the file with ChaCha20‑Poly1305 using that key.
3. Authenticates to Blossom with a kind‑24242 auth event (BUD‑01) and uploads the encrypted blob (BUD‑02).
4. Builds a Mostro DM payload JSON with Blossom URL and crypto metadata.
5. Sends a **shared-key custom wrap** (NIP-44 inner content, NIP-59 GiftWrap addressed to the shared key’s public key) via `send_admin_chat_message_via_shared_key`, so both the sender and the admin can decrypt the attachment metadata and fetch the blob.

