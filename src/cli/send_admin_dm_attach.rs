use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use nostr_sdk::prelude::*;
use rand_core::OsRng;
use rand_core::RngCore;
use uuid::Uuid;

use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};

const MAX_FILE_SIZE_BYTES: u64 = 25 * 1024 * 1024;

const BLOSSOM_SERVERS: &[&str] = &[
    "https://blossom.primal.net",
    "https://blossom.band",
    "https://nostr.media",
    "https://blossom.sector01.com",
    "https://24242.io",
    "https://otherstuff.shaving.kiwi",
    "https://blossom.f7z.io",
    "https://nosto.re",
    "https://blossom.poster.place",
];

fn derive_shared_key(trade_keys: &Keys, admin_pubkey: &PublicKey) -> [u8; 32] {
    use bitcoin::secp256k1::ecdh::shared_secret_point;
    use bitcoin::secp256k1::{Parity, PublicKey as SecpPublicKey};

    let sk = trade_keys.secret_key();
    let xonly = admin_pubkey
        .xonly()
        .expect("failed to get x-only public key for admin");
    let secp_pk = SecpPublicKey::from_x_only_public_key(xonly, Parity::Even);
    let mut point_bytes = shared_secret_point(&secp_pk, &sk).as_slice().to_vec();
    point_bytes.resize(32, 0);
    point_bytes
        .try_into()
        .expect("shared secret point must be at least 32 bytes")
}

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

async fn upload_to_blossom(encrypted_blob: Vec<u8>) -> Result<String> {
    use reqwest::StatusCode;

    let client = reqwest::Client::new();

    for server in BLOSSOM_SERVERS {
        let url = format!("{}/upload", server.trim_end_matches('/'));
        let res = client
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .body(encrypted_blob.clone())
            .send()
            .await;

        match res {
            Ok(resp) if resp.status() == StatusCode::OK => {
                let body = resp
                    .text()
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to read blossom response: {e}"))?;
                if body.trim().starts_with("blossom://") {
                    return Ok(body.trim().to_string());
                }
            }
            Ok(resp) => {
                eprintln!(
                    "Blossom upload failed on {server} with status {}",
                    resp.status()
                );
            }
            Err(e) => {
                eprintln!("Blossom upload error on {server}: {e}");
            }
        }
    }

    Err(anyhow::anyhow!("all Blossom servers failed"))
}

pub async fn execute_send_admin_dm_attach(
    receiver: PublicKey,
    ctx: &Context,
    order_id: &Uuid,
    file_path: &PathBuf,
) -> Result<()> {
    println!("📎 Send Admin DM Attachment");
    println!("═══════════════════════════════════════");

    let metadata = fs::metadata(file_path)
        .map_err(|e| anyhow::anyhow!("failed to read file metadata: {e}"))?;
    if !metadata.is_file() {
        anyhow::bail!("path is not a regular file: {}", file_path.display());
    }
    if metadata.len() > MAX_FILE_SIZE_BYTES {
        anyhow::bail!(
            "file too large ({} bytes, max is {} bytes)",
            metadata.len(),
            MAX_FILE_SIZE_BYTES
        );
    }

    let file_bytes = fs::read(file_path)
        .map_err(|e| anyhow::anyhow!("failed to read file {}: {e}", file_path.display()))?;

    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;

    let trade_keys = match order.trade_keys.as_ref() {
        Some(trade_keys) => Keys::parse(trade_keys)?,
        None => anyhow::bail!("No trade_keys found for this order"),
    };

    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order ID",
        &order_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🔑 ",
        "Trade Pubkey",
        &trade_keys.public_key().to_hex(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Admin Pubkey",
        &receiver.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "📄 ",
        "File",
        &file_path.to_string_lossy(),
    ));
    table.add_row(create_emoji_field_row(
        "📏 ",
        "Size",
        &format!("{} bytes", file_bytes.len()),
    ));

    println!("{table}");
    println!("💡 Encrypting file and uploading to Blossom...\n");

    let shared_key = derive_shared_key(&trade_keys, &receiver);
    let (encrypted_blob, nonce_hex) = encrypt_blob(shared_key, &file_bytes)?;
    let encrypted_size = encrypted_blob.len();
    let blossom_url = upload_to_blossom(encrypted_blob).await?;

    let filename = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment.bin")
        .to_string();

    // Best-effort MIME type detection based on the file extension.
    // We keep this intentionally simple to avoid extra dependencies and
    // fall back to application/octet-stream when unknown.
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

    Ok(())
}
