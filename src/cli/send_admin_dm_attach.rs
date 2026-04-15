use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use bitcoin_hashes::sha256::Hash as Sha256Hash;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use nostr_sdk::prelude::*;
use rand::rngs::OsRng;
use rand::RngCore;
use uuid::Uuid;

use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::util::messaging::derive_shared_key_bytes;
use crate::util::send_admin_chat_message_via_shared_key;

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

/// Upload encrypted blob to a Blossom server.
/// Blossom BUD-01 requires kind 24242 with: content (human-readable), tags ["t","upload"],
/// ["expiration", "<future unix ts>"], ["x", "<sha256 hex>"].
async fn upload_to_blossom(trade_keys: &Keys, encrypted_blob: Vec<u8>) -> Result<String> {
    use reqwest::StatusCode;

    let client = reqwest::Client::new();
    let payload_hash = Sha256Hash::hash(&encrypted_blob);
    let payload_hex = payload_hash
        .to_byte_array()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

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

    let shared_key = derive_shared_key_bytes(&trade_keys, &receiver)?;
    let (encrypted_blob, nonce_hex) = encrypt_blob(shared_key, &file_bytes)?;
    let encrypted_size = encrypted_blob.len();
    let blossom_url = upload_to_blossom(&trade_keys, encrypted_blob).await?;

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

    send_admin_chat_message_via_shared_key(
        &ctx.client,
        &trade_keys,
        &Keys::new(SecretKey::from_slice(&shared_key)?),
        &content,
    )
    .await?;

    println!("✅ Encrypted attachment sent successfully to admin!");

    Ok(())
}
