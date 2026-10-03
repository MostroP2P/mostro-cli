//! `declarepayer` and `paymenthistory`: payer declaration and
//! payment-account history (protocol book, `payer_declaration.md`).

use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::{
    cli::Context,
    db::Order,
    parser::{
        common::{print_info_line, print_key_value, print_section_header},
        dms::{format_payment_history, parse_dm_events},
    },
    util::{
        fetch_payer_history_thresholds,
        payer::{canonical_payer, PayerMethod},
        print_dm_events, send_dm, wait_for_dm,
    },
};

/// The local copy of `order_id`.
async fn local_order(order_id: &Uuid, ctx: &Context) -> Result<Order> {
    Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))
}

/// Trade keys stored for `order`.
fn trade_keys_of(order: &Order) -> Result<Keys> {
    match order.trade_keys.as_ref() {
        Some(keys) => Ok(Keys::parse(keys)?),
        None => Err(anyhow::anyhow!("No trade_keys found for this order")),
    }
}

/// Heading and explanation for a `not_found` answer to `payment-history`.
/// Mostro answers the same way when the buyer never declared and when the
/// order is past success (the declaration was consumed), so only claim the
/// latter when the local copy says the trade completed.
pub(crate) fn not_found_guidance(local_status: Option<&str>) -> (&'static str, &'static str) {
    if local_status == Some(Status::Success.to_string().as_str()) {
        (
            "⚠️ Trade already completed",
            "The declaration was consumed at success; the history is no longer queryable.",
        )
    } else {
        (
            "⚠️ No payer declaration available",
            "The buyer did not declare a payment sender, or the order is past success.",
        )
    }
}

/// Send `action` for `order_id`, signed with the order's trade keys, and
/// return the events Mostro answered with plus the request id used.
async fn send_to_mostro(
    order_id: &Uuid,
    action: Action,
    payload: Option<Payload>,
    trade_keys: &Keys,
    ctx: &Context,
) -> Result<(std::collections::BTreeSet<Event>, u64)> {
    let request_id = Uuid::new_v4().as_u128() as u64;
    let message = Message::new_order(Some(*order_id), Some(request_id), None, action, payload)
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;
    let sent = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        trade_keys,
        &ctx.mostro_pubkey,
        message,
        None,
        false,
    );
    let events = wait_for_dm(ctx, Some(trade_keys), sent).await?;
    Ok((events, request_id))
}

/// The answer to our `payment-history` query among `messages`: the one
/// carrying our `request_id`, or else an unsolicited `payment-history` push
/// for the same order (it holds the same data), so a push that lands first
/// is never mistaken for the reply to another command.
pub(crate) fn pick_reply(
    messages: &[Message],
    request_id: u64,
    order_id: &Uuid,
) -> Option<MessageKind> {
    let kinds = messages.iter().map(|m| m.get_inner_message_kind());
    let mut push = None;
    for kind in kinds {
        if kind.request_id == Some(request_id) {
            return Some(kind.clone());
        }
        if push.is_none()
            && kind.request_id.is_none()
            && kind.id == Some(*order_id)
            && kind.action == Action::PaymentHistory
        {
            push = Some(kind.clone());
        }
    }
    push
}

/// Outcome of comparing the buyer's plaintext with the declared hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeclarationCheck {
    Match,
    Mismatch { computed: String },
}

/// Hash the details the buyer sent the seller and compare them with the
/// hash it declared to Mostro.
pub(crate) fn check_declared(
    declared_hash: &str,
    method: &str,
    fields: &[String],
) -> Result<DeclarationCheck> {
    let canonical = canonical_payer(PayerMethod::parse(method)?, fields)?;
    let computed = payment_hash(&canonical);
    Ok(if computed == declared_hash {
        DeclarationCheck::Match
    } else {
        DeclarationCheck::Mismatch { computed }
    })
}

/// Buyer: canonicalise the payer account, declare its hash for the order
/// and show the canonical string to send to the seller over the chat.
pub async fn execute_declare_payer(
    order_id: &Uuid,
    method: &str,
    fields: &[String],
    ctx: &Context,
) -> Result<()> {
    let method = PayerMethod::parse(method)?;
    let canonical = canonical_payer(method, fields)?;
    let hash = payment_hash(&canonical);

    print_section_header("🧾 Declare Payer");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("🏦", "Method", method.prefix());
    print_key_value("🔤", "Canonical", &canonical);
    print_key_value("#️⃣", "Payment hash", &hash);
    print_info_line(
        "💡",
        "Only the hash goes to Mostro. Send the canonical string to the seller over the chat.",
    );
    println!();

    let trade_keys = trade_keys_of(&local_order(order_id, ctx).await?)?;
    let payload = Payload::PayerDeclaration(PayerDeclaration::new(hash));
    let (events, request_id) = send_to_mostro(
        order_id,
        Action::DeclarePayer,
        Some(payload),
        &trade_keys,
        ctx,
    )
    .await?;
    print_dm_events(events, request_id, ctx, Some(&trade_keys)).await
}

/// Seller: ask Mostro for the history of the account the buyer declared.
/// With `method` and `fields` (the details the buyer sent over the chat),
/// also check them against the declared hash.
pub async fn execute_payment_history(
    order_id: &Uuid,
    method: Option<&str>,
    fields: &[String],
    ctx: &Context,
) -> Result<()> {
    print_section_header("📈 Payment History");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_info_line(
        "💡",
        "Asking Mostro for the buyer's payment-account history...",
    );
    println!();

    let order = local_order(order_id, ctx).await?;
    let trade_keys = trade_keys_of(&order)?;
    let (events, request_id) =
        send_to_mostro(order_id, Action::PaymentHistory, None, &trade_keys, ctx).await?;
    let messages: Vec<Message> = parse_dm_events(events, &trade_keys, None, true)
        .await
        .into_iter()
        .map(|(message, _, _)| message)
        .collect();
    let reply = pick_reply(&messages, request_id, order_id)
        .ok_or_else(|| anyhow::anyhow!("No payment-history reply received from Mostro"))?;

    match reply.payload {
        Some(Payload::PaymentHistory(history)) => {
            let thresholds = fetch_payer_history_thresholds(ctx).await;
            println!("{}", format_payment_history(&history, thresholds));
            if let Some(method) = method {
                println!();
                match check_declared(&history.payment_hash, method, fields)? {
                    DeclarationCheck::Match => print_info_line(
                        "✅",
                        "The details the buyer sent you match the declared hash.",
                    ),
                    DeclarationCheck::Mismatch { computed } => {
                        print_section_header("🚨 DECLARATION MISMATCH");
                        println!("#️⃣ Hash of the details you received: {computed}");
                        println!(
                            "#️⃣ Hash the buyer declared:          {}",
                            history.payment_hash
                        );
                        println!("⚠️  The history above belongs to a DIFFERENT account than the one the buyer sent you.");
                        println!("⚠️  Do not release. Ask the buyer, and open a dispute if it is not resolved.");
                        return Err(anyhow::anyhow!(
                            "payer details do not match the declared hash"
                        ));
                    }
                }
            }
            Ok(())
        }
        Some(Payload::CantDo(Some(CantDoReason::NotFound))) => {
            // No history was retrieved whatever the cause, so fail for scripts.
            let (heading, detail) = not_found_guidance(order.status.as_deref());
            print_section_header(heading);
            println!("💡 {detail}");
            println!("💡 Sender verification is unavailable for this trade.");
            Err(anyhow::anyhow!(
                "no payer declaration available for order {order_id}"
            ))
        }
        Some(Payload::CantDo(reason)) => Err(anyhow::anyhow!(
            "Mostro refused the query: {:?}",
            reason.unwrap_or(CantDoReason::InvalidAction)
        )),
        other => Err(anyhow::anyhow!(
            "Unexpected payment-history reply: {:?}",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(request_id: Option<u64>, order: Uuid, action: Action) -> Message {
        Message::new_order(Some(order), request_id, None, action, None)
    }

    #[test]
    fn pick_reply_prefers_our_request_id() {
        let order = Uuid::new_v4();
        let messages = vec![
            kind(None, order, Action::PaymentHistory),
            kind(Some(7), order, Action::CantDo),
        ];
        let reply = pick_reply(&messages, 7, &order).unwrap();
        assert_eq!(reply.request_id, Some(7));
        assert_eq!(reply.action, Action::CantDo);
    }

    #[test]
    fn pick_reply_falls_back_to_a_history_push_for_the_same_order() {
        let order = Uuid::new_v4();
        let messages = vec![
            kind(None, Uuid::new_v4(), Action::PaymentHistory),
            kind(None, order, Action::FiatSentOk),
            kind(None, order, Action::PaymentHistory),
        ];
        let reply = pick_reply(&messages, 7, &order).unwrap();
        assert_eq!(reply.action, Action::PaymentHistory);
        assert_eq!(reply.id, Some(order));
    }

    #[test]
    fn pick_reply_ignores_unrelated_messages() {
        let order = Uuid::new_v4();
        let messages = vec![
            kind(Some(8), order, Action::PaymentHistory),
            kind(None, order, Action::FiatSentOk),
        ];
        assert!(pick_reply(&messages, 7, &order).is_none());
    }

    #[test]
    fn check_declared_detects_a_swapped_account() {
        let declared = payment_hash(
            &canonical_payer(
                PayerMethod::EuSepa,
                &["DE89370400440532013000".into(), "Alice Smith".into()],
            )
            .unwrap(),
        );
        let same = check_declared(
            &declared,
            "EU|SEPA",
            &["de89 3704 0044 0532 0130 00".into(), "alice  smith".into()],
        )
        .unwrap();
        assert_eq!(same, DeclarationCheck::Match);

        let other = check_declared(
            &declared,
            "EU|SEPA",
            &["DE44500105175407324931".into(), "Mallory Doe".into()],
        )
        .unwrap();
        assert!(matches!(other, DeclarationCheck::Mismatch { .. }));
    }

    #[test]
    fn not_found_guidance_only_claims_completion_from_the_local_status() {
        let (done, _) = not_found_guidance(Some("success"));
        assert!(done.contains("completed"));
        for status in [None, Some("fiat-sent"), Some("active")] {
            let (heading, detail) = not_found_guidance(status);
            assert!(heading.contains("No payer declaration"), "{status:?}");
            assert!(detail.contains("or the order is past success"));
        }
    }
}
