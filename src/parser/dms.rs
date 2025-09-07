use base64::engine::general_purpose;
use base64::Engine;
use mostro_core::prelude::*;
use nip44::v2::{decrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;

pub async fn parse_dm_events(events: Events, pubkey: &Keys) -> Vec<(Message, u64)> {
    let mut id_list = Vec::<EventId>::new();
    let mut direct_messages: Vec<(Message, u64)> = Vec::new();

    for dm in events.iter() {
        if !id_list.contains(&dm.id) {
            id_list.push(dm.id);

            let (created_at, message) = match dm.kind {
                nostr_sdk::Kind::GiftWrap => {
                    let unwrapped_gift = match nip59::extract_rumor(pubkey, dm).await {
                        Ok(u) => u,
                        Err(_) => {
                            println!("Error unwrapping gift");
                            continue;
                        }
                    };
                    let (message, _): (Message, Option<String>) =
                        serde_json::from_str(&unwrapped_gift.rumor.content).unwrap();
                    (unwrapped_gift.rumor.created_at, message)
                }
                nostr_sdk::Kind::PrivateDirectMessage => {
                    let ck =
                        if let Ok(ck) = ConversationKey::derive(pubkey.secret_key(), &dm.pubkey) {
                            ck
                        } else {
                            continue;
                        };
                    let b64decoded_content =
                        match general_purpose::STANDARD.decode(dm.content.as_bytes()) {
                            Ok(b64decoded_content) => b64decoded_content,
                            Err(_) => {
                                continue;
                            }
                        };
                    let unencrypted_content = match decrypt_to_bytes(&ck, &b64decoded_content) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            continue;
                        }
                    };
                    let message_str = match String::from_utf8(unencrypted_content) {
                        Ok(s) => s,
                        Err(_) => {
                            continue;
                        }
                    };
                    let message = match Message::from_json(&message_str) {
                        Ok(m) => m,
                        Err(_) => {
                            continue;
                        }
                    };
                    (dm.created_at, message)
                }
                _ => continue,
            };

            let since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::minutes(30))
                .unwrap()
                .timestamp() as u64;
            if created_at.as_u64() < since_time {
                continue;
            }
            direct_messages.push((message, created_at.as_u64()));
        }
    }
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));
    direct_messages
}
