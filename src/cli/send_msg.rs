use crate::cli::Commands;
use crate::util::get_keys;
use crate::util::send_order_id_cmd;

use anyhow::Result;
use log::info;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use std::process;
use uuid::Uuid;

pub async fn execute_send_msg(
    command: Commands,
    order_id: Option<Uuid>,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
    text: Option<&str>,
) -> Result<()> {
    // Get desised action based on command from CLI
    let requested_action = match command {
        Commands::FiatSent { order_id: _ } => Action::FiatSent,
        Commands::Release { order_id: _ } => Action::Release,
        Commands::Cancel { order_id: _ } => Action::Cancel,
        Commands::Dispute { order_id: _ } => Action::Dispute,
        Commands::AdmCancel { order_id: _ } => Action::AdminCancel,
        Commands::AdmSettle { order_id: _ } => Action::AdminSettle,
        Commands::AdmAddSolver { npubkey: _ } => Action::AdminAddSolver,
        _ => {
            println!("Not a valid command!");
            process::exit(0);
        }
    };

    println!(
        "Sending {} command for order {:?} to mostro pubId {}",
        requested_action,
        order_id,
        mostro_key.clone()
    );
    let keys = get_keys()?;
    let mut content = None;
    if let Some(t) = text {
        content = Some(Content::TextMessage(t.to_string()));
    }

    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    // Create message
    let message = Message::new_order(order_id, Some(master_pubkey), requested_action, content)
        .as_json()
        .unwrap();
    info!("Sending message: {:#?}", message);
    send_order_id_cmd(client, my_key, mostro_key, message, false).await?;

    Ok(())
}
