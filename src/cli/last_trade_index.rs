use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    db::User,
    parser::common::{
        print_key_value, print_section_header, print_success_message, print_trade_index,
    },
    parser::{dms::print_commands_results, parse_dm_events},
    util::{send_dm, wait_for_dm},
};

pub async fn execute_last_trade_index(
    identity_keys: &Keys,
    mostro_key: PublicKey,
    ctx: &Context,
) -> Result<()> {
    let kind = MessageKind::new(None, None, None, Action::LastTradeIndex, None);
    let last_trade_index_message = Message::Restore(kind);
    let message_json = last_trade_index_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the last trade index message to Mostro server
    let sent_message = send_dm(
        &ctx.client,
        Some(identity_keys),
        &ctx.trade_keys,
        &mostro_key,
        message_json,
        None,
        false,
    );

    // Log the sent message
    print_section_header("🔢 Last Trade Index Request");
    print_key_value("👤", "User", &identity_keys.public_key().to_string());
    print_key_value("🎯", "Target", &mostro_key.to_string());
    print_key_value("💡", "Action", "Requesting last trade index from Mostro...");
    println!();

    // Wait for incoming DM
    let recv_event = wait_for_dm(ctx, Some(&ctx.trade_keys), sent_message).await?;

    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, &ctx.trade_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message = message.get_inner_message_kind();
        if message.action == Action::LastTradeIndex {
            print_commands_results(message, ctx).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Received response with mismatched action. Expected: {:?}, Got: {:?}",
                Action::LastTradeIndex,
                message.action
            ));
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }

    Ok(())
}

/// Print the private key corresponding to the last trade index
/// stored locally for this user.
pub async fn execute_last_trade_index_private_key(ctx: &Context) -> Result<()> {
    // Get the last known trade index from the local database
    let last_trade_index = User::get_last_trade_index(ctx.pool.clone()).await?;

    print_section_header("🔑 Last Trade Index Private Key");
    print_trade_index(last_trade_index as u64);

    // Derive the trade keys for this index from the user's mnemonic
    let trade_keys = User::get_trade_keys(&ctx.pool, last_trade_index).await?;
    let sk_hex = trade_keys.secret_key().to_secret_hex();
    let pk_str = trade_keys.public_key().to_string();

    print_key_value("🔐", "Private Key (hex)", &sk_hex);
    print_key_value("🔓", "Public Key", &pk_str);
    print_success_message("Derived last trade index keypair from local mnemonic.");

    Ok(())
}
