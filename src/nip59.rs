use base64::engine::{general_purpose, Engine};
use nip44::v2::{decrypt_to_bytes, encrypt_to_bytes, ConversationKey};
use nostr_sdk::event::builder::Error as BuilderError;
use nostr_sdk::prelude::*;


pub fn gift_wrap_from_seal(
    receiver: &PublicKey,
    seal: &Event,
    expiration: Option<Timestamp>,
    pow: u8,
) -> Result<Event, BuilderError> {
    let ephemeral_keys: Keys = Keys::generate();
    // Derive conversation key
    let ck = ConversationKey::derive(ephemeral_keys.secret_key(), receiver);
    // Encrypt payload
    let encrypted_content = encrypt_to_bytes(&ck, seal.as_json())?;
    let mut tags: Vec<Tag> = Vec::with_capacity(1 + usize::from(expiration.is_some()));
    tags.push(Tag::public_key(*receiver));

    if let Some(timestamp) = expiration {
        tags.push(Tag::expiration(timestamp));
    }
    let tags = Tags::new(tags);
    // Encode with base64
    let b64decoded_content = general_purpose::STANDARD.encode(encrypted_content);
    let event = EventBuilder::new(Kind::GiftWrap, b64decoded_content)
        .tags(tags)
        .custom_created_at(Timestamp::tweaked(nip59::RANGE_RANDOM_TIMESTAMP_TWEAK))
        .pow(pow)
        .build(ephemeral_keys.public_key())
        .sign_with_keys(&ephemeral_keys)?;

    Ok(event)
}

pub fn unwrap_gift_wrap(
    keys: Option<&Keys>,
    gw_ck: Option<ConversationKey>,
    seal_ck: Option<ConversationKey>,
    gift_wrap: &Event,
) -> Result<UnwrappedGift, BuilderError> {
    let gw_ck = match keys {
        Some(keys) => ConversationKey::derive(keys.secret_key(), &gift_wrap.pubkey),
        None => match gw_ck {
            Some(ck) => ck,
            None => {
                return Err(BuilderError::NIP44(
                    nostr_sdk::nips::nip44::Error::NotFound(
                        "No keys or conversation key".to_string(),
                    ),
                ))
            }
        },
    };
    let b64decoded_content = match general_purpose::STANDARD.decode(gift_wrap.content.as_bytes()) {
        Ok(b64decoded_content) => b64decoded_content,
        Err(e) => {
            return Err(BuilderError::NIP44(
                nostr_sdk::nips::nip44::Error::NotFound(e.to_string()),
            ));
        }
    };
    // Decrypt and verify seal
    let seal = decrypt_to_bytes(&gw_ck, b64decoded_content)?;
    let seal = String::from_utf8(seal).expect("Found invalid UTF-8");
    let seal = match Event::from_json(seal) {
        Ok(seal) => seal,
        Err(e) => {
            println!("Error: {:#?}", e);
            return Err(BuilderError::NIP44(
                nostr_sdk::nips::nip44::Error::NotFound(e.to_string()),
            ));
        }
    };
    let seal_ck = match keys {
        Some(keys) => ConversationKey::derive(keys.secret_key(), &seal.pubkey),
        None => match seal_ck {
            Some(ck) => ck,
            None => {
                return Err(BuilderError::NIP44(
                    nostr_sdk::nips::nip44::Error::NotFound(
                        "No keys or conversation key".to_string(),
                    ),
                ))
            }
        },
    };
    let b64decoded_content = match general_purpose::STANDARD.decode(seal.content.as_bytes()) {
        Ok(b64decoded_content) => b64decoded_content,
        Err(e) => {
            return Err(BuilderError::NIP44(
                nostr_sdk::nips::nip44::Error::NotFound(e.to_string()),
            ))
        }
    };
    // Decrypt rumor
    let rumor = decrypt_to_bytes(&seal_ck, b64decoded_content)?;
    let rumor = String::from_utf8(rumor).expect("Found invalid UTF-8");

    Ok(UnwrappedGift {
        sender: seal.pubkey,
        rumor: UnsignedEvent::from_json(rumor)?,
    })
}
