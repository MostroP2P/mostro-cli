use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
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
        identity_keys,
        &mostro_key,
        message_json,
        None,
        false,
    );

    // Log the sent message
    println!(
        "Sent request to Mostro to get last trade index of user {}",
        identity_keys.public_key()
    );

    // Wait for incoming DM
    let recv_event = wait_for_dm(ctx, Some(identity_keys), sent_message).await?;

    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, &identity_keys, None).await;
    if let Some((message, _, _)) = messages.first() {
        let message = message.get_inner_message_kind();
        if message.action == Action::LastTradeIndex {
            print_commands_results(message, None, ctx).await?
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
