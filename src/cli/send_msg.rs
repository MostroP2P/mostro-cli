use crate::cli::Commands;
use crate::util::get_keys;
use crate::util::send_order_id_cmd;
use anyhow::Result;
use mostro_core::Action;
use mostro_core::Message as MostroMessage;
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use std::process;
use uuid::Uuid;

pub async fn execute_send_msg(
    command: Commands,
    order_id: &Uuid,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    // Get desised action based on command from CLI
    let requested_action = match command {
        Commands::FiatSent { order_id: _ } => Action::FiatSent,
        Commands::Release { order_id: _ } => Action::Release,
        Commands::Cancel { order_id: _ } => Action::Cancel,
        Commands::Dispute { order_id: _ } => Action::Dispute,
        _ => {
            println!("Not a valid command!");
            process::exit(0);
        }
    };

    println!(
        "Sending {} command for order {} to mostro pubId {}",
        requested_action,
        order_id,
        mostro_key.clone()
    );
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    // Create fiat sent message
    let message = MostroMessage::new(
        0,
        Some(*order_id),
        Some(master_pubkey),
        requested_action,
        None,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, message, false).await?;
    Ok(())
}
