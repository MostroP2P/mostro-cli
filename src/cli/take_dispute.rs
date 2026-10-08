use crate::util::messaging::get_admin_keys;
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::{Keys, PublicKey, ToBech32};
use uuid::Uuid;

use crate::{
    cli::Context,
    parser::common::{create_emoji_field_row, create_field_value_header, create_standard_table},
    parser::{dms::print_commands_results, parse_dm_events},
    util::{send_dm, send_dm_with_id, wait_for_dm, wait_for_reply_to},
};

/// Solver categories mostrod accepts after the `:` separator.
const SOLVER_CATEGORIES: [&str; 3] = ["read", "read-write", "write"];

/// Normalize the `admaddsolver` argument (`<pubkey>[:<category>]`) into the
/// `npub[:category]` payload mostrod expects. mostrod only parses bech32, so a
/// hex key is converted here instead of being silently rejected by the daemon.
fn normalize_solver_payload(input: &str) -> Result<String> {
    let mut parts = input.trim().split(':');
    let raw_pubkey = parts.next().unwrap_or_default().trim();
    let pubkey = PublicKey::parse(raw_pubkey).map_err(|_| {
        anyhow::anyhow!("Invalid solver pubkey '{raw_pubkey}': expected an npub or 64-char hex key")
    })?;
    let npub = pubkey
        .to_bech32()
        .map_err(|e| anyhow::anyhow!("Failed to encode solver pubkey as npub: {e}"))?;

    let category = parts.next().map(str::trim);
    if parts.next().is_some() {
        return Err(anyhow::anyhow!(
            "Invalid solver argument '{input}': expected <pubkey>[:<category>]"
        ));
    }

    match category {
        None => Ok(npub),
        Some(c) if SOLVER_CATEGORIES.contains(&c) => Ok(format!("{npub}:{c}")),
        Some(c) => Err(anyhow::anyhow!(
            "Invalid solver category '{c}': expected one of {}",
            SOLVER_CATEGORIES.join(", ")
        )),
    }
}

/// mostrod only accepts `AdminAddSolver` signed by its own key, and drops any
/// other sender without replying, so catch a wrong ADMIN_NSEC before sending.
fn ensure_admin_is_mostro(admin_keys: &Keys, mostro_pubkey: &PublicKey) -> Result<()> {
    if admin_keys.public_key() == *mostro_pubkey {
        return Ok(());
    }
    let to_npub = |pk: PublicKey| pk.to_bech32().unwrap_or_else(|_| pk.to_hex());
    Err(anyhow::anyhow!(
        "ADMIN_NSEC does not match the Mostro key: admin pubkey is {} but Mostro is {}. \
         Only the Mostro daemon key can add solvers.",
        to_npub(admin_keys.public_key()),
        to_npub(*mostro_pubkey)
    ))
}

pub async fn execute_admin_add_solver(npubkey: &str, ctx: &Context) -> Result<()> {
    println!("👑 Admin Add Solver");
    println!("═══════════════════════════════════════");
    let payload = normalize_solver_payload(npubkey)?;
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row("🔑 ", "Solver PubKey", &payload));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");

    let admin_keys = get_admin_keys(ctx)?;
    ensure_admin_is_mostro(admin_keys, &ctx.mostro_pubkey)?;

    println!("💡 Adding new solver to Mostro...\n");

    // Tag the request so the confirmation can be matched to it: mostrod
    // echoes `request_id` back in its reply.
    let request_id = Uuid::new_v4().as_u64_pair().0;
    let add_solver_message = Message::new_dispute(
        Some(Uuid::new_v4()),
        Some(request_id),
        None,
        Action::AdminAddSolver,
        Some(Payload::TextMessage(payload)),
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Wait for Mostro's confirmation: mostrod replies only after the solver
    // is stored and stays silent on internal errors, so no reply means the
    // solver was not added.
    // ADMIN_NSEC is Mostro's own key here, so the request itself matches the
    // reply filter; `wait_for_reply_to` skips it by event id.
    let sent_message = send_dm_with_id(
        &ctx.client,
        admin_keys,
        admin_keys,
        &ctx.mostro_pubkey,
        add_solver_message,
        None,
        false,
    );

    let recv_event = wait_for_reply_to(ctx, Some(admin_keys), sent_message)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Solver was NOT added: no confirmation from Mostro ({e}). \
                 Check the mostrod logs for the cause."
            )
        })?;

    let messages = parse_dm_events(recv_event, admin_keys, None, true).await;
    let (message, _, sender_pubkey) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("Solver was NOT added: no response received from Mostro"))?;

    if *sender_pubkey != ctx.mostro_pubkey {
        return Err(anyhow::anyhow!("Received response from wrong sender"));
    }

    let message_kind = message.get_inner_message_kind();
    if message_kind.request_id != Some(request_id) {
        return Err(anyhow::anyhow!(
            "Solver was NOT confirmed: Mostro's reply does not match this request"
        ));
    }
    match message_kind.action {
        Action::AdminAddSolver => {
            println!("✅ Solver added successfully!");
            Ok(())
        }
        Action::CantDo => print_commands_results(message_kind, ctx)
            .await
            .map_err(|e| anyhow::anyhow!("Solver was NOT added: {e}")),
        ref other => Err(anyhow::anyhow!(
            "Solver was NOT added: unexpected response from Mostro. Expected: {:?}, Got: {:?}",
            Action::AdminAddSolver,
            other
        )),
    }
}

pub async fn execute_admin_cancel_dispute(
    dispute_id: &Uuid,
    slash_seller: bool,
    slash_buyer: bool,
    ctx: &Context,
) -> Result<()> {
    println!("👑 Admin Cancel Dispute");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    if slash_seller {
        table.add_row(create_emoji_field_row("⚔️  ", "Slash", "seller bond"));
    }
    if slash_buyer {
        table.add_row(create_emoji_field_row("⚔️  ", "Slash", "buyer bond"));
    }
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Canceling dispute...\n");

    let admin_keys = get_admin_keys(ctx)?;

    let payload = if slash_seller || slash_buyer {
        Some(Payload::BondResolution(BondResolution {
            slash_seller,
            slash_buyer,
        }))
    } else {
        None
    };

    let admin_cancel_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminCancel, payload)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the message and await Mostro's reply so the success message is
    // only printed when the cancel actually went through. Admin identity
    // binds via the seal/rumor signers — the admin role doesn't rotate
    // trade keys, so `admin_keys` signs both layers.
    let sent_message = send_dm(
        &ctx.client,
        admin_keys,
        admin_keys,
        &ctx.mostro_pubkey,
        admin_cancel_message,
        None,
        false,
    );

    let recv_event = wait_for_dm(ctx, Some(admin_keys), sent_message)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to receive response from Mostro for AdminCancel: {e}. \
                 The operation may not have completed; verify the order id exists and check backend logs."
            )
        })?;

    let messages = parse_dm_events(recv_event, admin_keys, None, true).await;
    let (message, _, sender_pubkey) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("No response received from Mostro"))?;

    if *sender_pubkey != ctx.mostro_pubkey {
        return Err(anyhow::anyhow!("Received response from wrong sender"));
    }

    let message_kind = message.get_inner_message_kind();
    if message_kind.action == Action::AdminCanceled {
        println!("✅ Dispute canceled successfully!");
        Ok(())
    } else if message_kind.action == Action::CantDo {
        print_commands_results(message_kind, ctx).await
    } else {
        Err(anyhow::anyhow!(
            "Received response with mismatched action. Expected: {:?}, Got: {:?}",
            Action::AdminCanceled,
            message_kind.action
        ))
    }
}

pub async fn execute_admin_settle_dispute(
    dispute_id: &Uuid,
    slash_seller: bool,
    slash_buyer: bool,
    ctx: &Context,
) -> Result<()> {
    println!("👑 Admin Settle Dispute");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    if slash_seller {
        table.add_row(create_emoji_field_row("⚔️  ", "Slash", "seller bond"));
    }
    if slash_buyer {
        table.add_row(create_emoji_field_row("⚔️  ", "Slash", "buyer bond"));
    }
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Settling dispute...\n");

    let admin_keys = get_admin_keys(ctx)?;

    let payload = if slash_seller || slash_buyer {
        Some(Payload::BondResolution(BondResolution {
            slash_seller,
            slash_buyer,
        }))
    } else {
        None
    };

    let admin_settle_message =
        Message::new_dispute(Some(*dispute_id), None, None, Action::AdminSettle, payload)
            .as_json()
            .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the message and await Mostro's reply so the success message is
    // only printed when the settle actually went through. Admin identity
    // binds via the seal/rumor signers — the admin role doesn't rotate
    // trade keys, so `admin_keys` signs both layers.
    let sent_message = send_dm(
        &ctx.client,
        admin_keys,
        admin_keys,
        &ctx.mostro_pubkey,
        admin_settle_message,
        None,
        false,
    );

    let recv_event = wait_for_dm(ctx, Some(admin_keys), sent_message)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to receive response from Mostro for AdminSettle: {e}. \
                 The operation may not have completed; verify the order id exists and check backend logs."
            )
        })?;

    let messages = parse_dm_events(recv_event, admin_keys, None, true).await;
    let (message, _, sender_pubkey) = messages
        .first()
        .ok_or_else(|| anyhow::anyhow!("No response received from Mostro"))?;

    if *sender_pubkey != ctx.mostro_pubkey {
        return Err(anyhow::anyhow!("Received response from wrong sender"));
    }

    let message_kind = message.get_inner_message_kind();
    if message_kind.action == Action::AdminSettled {
        println!("✅ Dispute settled successfully!");
        Ok(())
    } else if message_kind.action == Action::CantDo {
        print_commands_results(message_kind, ctx).await
    } else {
        Err(anyhow::anyhow!(
            "Received response with mismatched action. Expected: {:?}, Got: {:?}",
            Action::AdminSettled,
            message_kind.action
        ))
    }
}

pub async fn execute_take_dispute(dispute_id: &Uuid, ctx: &Context) -> Result<()> {
    println!("👑 Admin Take Dispute");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🆔 ",
        "Dispute ID",
        &dispute_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Taking dispute...\n");

    let admin_keys = get_admin_keys(ctx)?;

    // Build admin dispute message
    let take_dispute_message = Message::new_dispute(
        Some(*dispute_id),
        None,
        None,
        Action::AdminTakeDispute,
        None,
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the dispute message and wait for response. Admin identity
    // binds via the rumor/seal/inner-signature produced from `admin_keys`.
    // The admin role doesn't rotate trade keys, so the same key signs both
    // the seal and the rumor (full-privacy-style wrap).
    let sent_message = send_dm(
        &ctx.client,
        admin_keys,
        admin_keys,
        &ctx.mostro_pubkey,
        take_dispute_message,
        None,
        false,
    );

    // Wait for incoming DM response
    let recv_event = wait_for_dm(ctx, Some(admin_keys), sent_message).await?;

    // Parse the incoming DM
    let messages = parse_dm_events(recv_event, admin_keys, None, true).await;
    if let Some((message, _, sender_pubkey)) = messages.first() {
        let message_kind = message.get_inner_message_kind();
        if *sender_pubkey != ctx.mostro_pubkey {
            return Err(anyhow::anyhow!("Received response from wrong sender"));
        }
        if message_kind.action == Action::AdminTookDispute {
            print_commands_results(message_kind, ctx).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Received response with mismatched action. Expected: {:?}, Got: {:?}",
                Action::AdminTookDispute,
                message_kind.action
            ));
        }
    } else {
        return Err(anyhow::anyhow!("No response received from Mostro"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLVER_HEX: &str = "cff1d9c8e6dd45b7ecedf13fc38068cd9d209ef1b8f636adfd87f19f104e2112";
    const SOLVER_NPUB: &str = "npub1elcanj8xm4zm0m8d7ylu8qrgekwjp8h3hrmrdt0aslce7yzwyyfq93cdv2";

    #[test]
    fn normalize_solver_payload_converts_hex_to_npub() {
        assert_eq!(normalize_solver_payload(SOLVER_HEX).unwrap(), SOLVER_NPUB);
    }

    #[test]
    fn normalize_solver_payload_keeps_npub() {
        assert_eq!(normalize_solver_payload(SOLVER_NPUB).unwrap(), SOLVER_NPUB);
    }

    #[test]
    fn normalize_solver_payload_keeps_category_suffix() {
        let input = format!("{SOLVER_HEX}:read");
        assert_eq!(
            normalize_solver_payload(&input).unwrap(),
            format!("{SOLVER_NPUB}:read")
        );
        let input = format!(" {SOLVER_NPUB} : read-write ");
        assert_eq!(
            normalize_solver_payload(&input).unwrap(),
            format!("{SOLVER_NPUB}:read-write")
        );
    }

    #[test]
    fn normalize_solver_payload_rejects_invalid_pubkey() {
        assert!(normalize_solver_payload("not-a-key").is_err());
        assert!(normalize_solver_payload("").is_err());
        assert!(normalize_solver_payload(&SOLVER_HEX[..60]).is_err());
    }

    #[test]
    fn normalize_solver_payload_rejects_unknown_category() {
        assert!(normalize_solver_payload(&format!("{SOLVER_NPUB}:admin")).is_err());
        assert!(normalize_solver_payload(&format!("{SOLVER_NPUB}:")).is_err());
        assert!(normalize_solver_payload(&format!("{SOLVER_NPUB}:read:write")).is_err());
    }

    #[test]
    fn ensure_admin_is_mostro_accepts_mostro_key() {
        let mostro = Keys::generate();
        assert!(ensure_admin_is_mostro(&mostro, &mostro.public_key()).is_ok());
    }

    #[test]
    fn ensure_admin_is_mostro_rejects_other_key() {
        let mostro = Keys::generate();
        let other = Keys::generate();
        let err = ensure_admin_is_mostro(&other, &mostro.public_key()).unwrap_err();
        let expected = mostro.public_key().to_bech32().unwrap();
        assert!(err.to_string().contains(&expected));
    }
}
