use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

use crate::{
    cli::Context,
    parser::common::{print_key_value, print_section_header},
    parser::{dms::print_commands_results, parse_dm_events},
    util::{ensure_first_contact_pow, send_dm, wait_for_dm_matching},
};

/// Ask Mostro for the reputation it holds for our identity key.
///
/// Sent from a trade key with the identity proof (Mostro rejects identity ==
/// sender). Because that trade key is not in an active order/dispute, the
/// daemon's `pow_first_contact` lane applies: [`ensure_first_contact_pow`]
/// fails fast when configured `POW` is too low. The reply is correlated with
/// [`wait_for_dm_matching`] so an unrelated DM to the same trade key cannot
/// be mistaken for this request's `UserInfo` / `CantDo` answer.
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
    // That trade key is not in an active order/dispute, so Mostro's
    // pow_first_contact lane applies — fail fast if POW is too low.
    ensure_first_contact_pow(ctx).await?;

    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Correlate by request_id + action: keep waiting past unrelated Mostro DMs
    // (including uncorrelated CantDo) until UserInfo/CantDo echoes this id.
    // Do not use validate_response here — it returns MostroCantDo before
    // checking request_id.
    let recv_event = wait_for_dm_matching(
        ctx,
        Some(&ctx.trade_keys),
        request_id,
        &[Action::UserInfo, Action::CantDo],
        sent_message,
    )
    .await?;

    let messages = parse_dm_events(recv_event, &ctx.trade_keys, None, true).await;
    let (message, _, _) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("No response received from Mostro"))?;
    let message = message.get_inner_message_kind();
    // Defense in depth: wait_for_dm_matching already enforced this filter.
    if message.request_id != Some(request_id)
        || !matches!(message.action, Action::UserInfo | Action::CantDo)
    {
        return Err(anyhow::anyhow!(
            "Received response that does not match this user-info request \
             (request_id={:?}, action={:?})",
            message.request_id,
            message.action
        ));
    }
    print_commands_results(message, ctx).await
}
