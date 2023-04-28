use anyhow::Result;
use mostro_core::Message as MostroMessage;
use mostro_core::{Action, Content};
use nostr_sdk::{Client, Keys};
use uuid::Uuid;

use crate::util::send_order_id_cmd;

pub async fn execute_rate_user(
    order_id: &Uuid,
    rating: &u64,
    my_key: &Keys,
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
    let voting_message = MostroMessage::new(
        0,
        Some(*order_id),
        None,
        Action::RateUser,
        Some(rating_content),
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, my_key.public_key(), voting_message, true).await?;

    std::process::exit(0);
}
