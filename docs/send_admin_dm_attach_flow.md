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

### 5. DM payload and gift wrap to admin

Once the encrypted blob is uploaded and we have `blossom_url`, the DM payload is built:

```268:311:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
let filename = file_path
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("attachment.bin")
    .to_string();

// Best-effort MIME type detection based on the file extension.
let mime_type = file_path
    .extension()
    .and_then(|ext| ext.to_str())
    .map(|ext| ext.to_ascii_lowercase())
    .map(|ext| match ext.as_str() {
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    })
    .unwrap_or("application/octet-stream")
    .to_string();

let payload_json = serde_json::json!({
    "type": "file_encrypted",
    "blossom_url": blossom_url,
    "nonce": nonce_hex,
    "mime_type": mime_type,
    "original_size": file_bytes.len(),
    "filename": filename,
    "encrypted_size": encrypted_size,
    "file_type": "document",
});

let content = serde_json::to_string(&payload_json)
    .map_err(|e| anyhow::anyhow!("failed to serialize attachment payload: {e}"))?;
```

This JSON is the **Mostro DM payload** describing the encrypted attachment:

- `type = "file_encrypted"` – payload kind.
- `blossom_url` – where the admin can fetch the encrypted blob.
- `nonce` – hex nonce for ChaCha20‑Poly1305.
- `mime_type` – hint about original file type.
- `original_size` / `encrypted_size` – size bookkeeping.
- `filename` – original filename.

#### 5.1 Gift‑wrapped DM event

```316:331:/home/pinballwizard/rust_prj/mostro_p2p/mostro-cli/src/cli/send_admin_dm_attach.rs
let pow: u8 = std::env::var("POW")
    .unwrap_or_else(|_| "0".to_string())
    .parse()
    .unwrap_or(0);

let rumor = EventBuilder::text_note(content)
    .pow(pow)
    .build(trade_keys.public_key());

let event = EventBuilder::gift_wrap(&trade_keys, &receiver, rumor, Tags::new()).await?;

ctx.client
    .send_event(&event)
    .await
    .map_err(|e| anyhow::anyhow!("failed to send gift wrap event: {e}"))?;

println!("✅ Encrypted attachment sent successfully to admin!");
```

- **Rumor event** (inner):
  - `kind`: `TextNote`
  - `content`: the `payload_json` string above.
  - `pubkey`: `trade_keys.public_key()` (per‑order trade identity).
  - Optional POW from `POW` env var.

- **Outer GiftWrap event** (NIP‑59):
  - Created via `EventBuilder::gift_wrap(&trade_keys, &receiver, rumor, Tags::new())`.
  - **Signer / sender**: `trade_keys` (per‑order).
  - **Recipient**: `receiver` (admin / solver Nostr pubkey).
  - **Tags**: currently empty (no extra expiration here; optional).

- **Relaying**:
  - Final event is sent with `ctx.client.send_event(&event).await`.
  - `ctx.client` is connected to configured Nostr relays via `RELAYS` env var.

### 6. Keys and protocols summary

- **Keys**:
  - `trade_keys` (per‑order):
    - Used for:
      - ECDH shared secret with admin pubkey for file encryption.
      - Nostr DM identity for the rumor + giftwrap.
      - (Indirectly) for signing Blossom auth event (kind 24242).
  - `receiver`:
    - Admin / solver Nostr pubkey; DM destination for the final NIP‑59 GiftWrap.

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
    - NIP‑13: optional POW on the rumor text note.
    - NIP‑40: optional expiration (not used on the DM here).
    - NIP‑59: GiftWrap envelope:
      - Wraps the text‑note rumor for the admin’s pubkey.

End‑to‑end, `sendadmindmattach`:

1. Derives a shared ECDH key between trade and admin.
2. Encrypts the file with ChaCha20‑Poly1305.
3. Authenticates to Blossom with a kind‑24242 auth event (BUD‑01) and uploads the encrypted blob (BUD‑02).
4. Builds a Mostro DM payload with Blossom URL + crypto metadata.
5. Sends a NIP‑59 gift‑wrapped text note from the trade keys to the admin pubkey with that payload as content.

