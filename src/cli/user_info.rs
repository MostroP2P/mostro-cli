use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

use crate::{
    cli::Context,
    parser::common::{print_key_value, print_section_header},
    parser::{dms::print_commands_results, parse_dm_events},
    util::{send_dm, wait_for_dm},
};

/// Ask Mostro for the reputation it holds for our identity key.
pub async fn execute_user_info(ctx: &Context) -> Result<()> {
    let request_id = Uuid::new_v4().as_u128() as u64;
    let kind = MessageKind::new(None, Some(request_id), None, Action::UserInfo, None);
    let message_json = Message::Restore(kind)
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    print_section_header("👤 User Info Request");
    print_key_value(
        "👤",
        "Identity",
        &ctx.identity_keys.public_key().to_string(),
    );
    print_key_value("🎯", "Target", &ctx.mostro_pubkey.to_string());
    print_key_value("💡", "Action", "Requesting reputation from Mostro...");
    println!();

    // The reputation belongs to the identity key, so the request is sent from
    // a trade key and carries the identity proof; Mostro rejects identity == sender.
    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    let recv_event = wait_for_dm(ctx, Some(&ctx.trade_keys), sent_message).await?;

    let messages = parse_dm_events(recv_event, &ctx.trade_keys, None, true).await;
    let (message, _, _) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("No response received from Mostro"))?;
    let message = message.get_inner_message_kind();
    match &message.action {
        Action::UserInfo | Action::CantDo => print_commands_results(message, ctx).await,
        other => Err(anyhow::anyhow!(
            "Received response with mismatched action. Expected: {:?}, Got: {:?}",
            Action::UserInfo,
            other
        )),
    }
}
