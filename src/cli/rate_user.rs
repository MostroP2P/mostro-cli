use anyhow::Result;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::send_order_id_cmd;

pub async fn execute_rate_user(
    order_id: &Uuid,
    rating: &u8,
    my_key: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    // User rating
    let rating_content;

    // Check boundaries
    if let 1..=5 = *rating {
        rating_content = Content::RatingUser(*rating);
    } else {
        println!("Rating must be in the range 1 - 5");
        std::process::exit(0);
    }

    // Create rating message of counterpart
    let rate_message = Message::new_order(Some(*order_id), Action::RateUser, Some(rating_content))
        .as_json()
        .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, rate_message, true, true).await?;

    std::process::exit(0);
}
