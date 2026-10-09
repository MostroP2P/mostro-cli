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
        dms::{format_payment_history, parse_dm_events, print_commands_results},
    },
    util::{
        fetch_payer_history_thresholds,
        messaging::parse_secret_env,
        payer::{canonical_payer, PayerMethod},
        send_dm, wait_for_dm, WaitForDmTimeout,
    },
};

/// The payer fields to use: the `-f` values, or, with `from_stdin`, one
/// field per line read from stdin, so bank details stay out of shell history
/// and the process list.
pub fn resolve_payer_fields(fields: &[String], from_stdin: bool) -> Result<Vec<String>> {
    if !from_stdin {
        return Ok(fields.to_vec());
    }
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprintln!("Enter one account field per line, then Ctrl-D:");
    }
    read_fields(stdin.lock())
}

/// A chat message read from stdin (trailing newline trimmed), for
/// `dmtouser --message-stdin`.
pub fn read_message_stdin() -> Result<String> {
    use std::io::{IsTerminal, Read};
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprintln!("Enter the message, then Ctrl-D:");
    }
    let mut text = String::new();
    stdin.read_to_string(&mut text)?;
    let text = text.trim_end_matches(['\n', '\r']).to_string();
    if text.trim().is_empty() {
        return Err(anyhow::anyhow!("empty message on stdin"));
    }
    Ok(text)
}

fn read_fields(input: impl std::io::BufRead) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for line in input.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            out.push(line);
        }
    }
    Ok(out)
}

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

/// Send `action` for `order_id` with `request_id`, signed with the order's
/// trade keys, and return the events Mostro answered with.
async fn send_to_mostro(
    order_id: &Uuid,
    request_id: u64,
    action: Action,
    payload: Option<Payload>,
    trade_keys: &Keys,
    ctx: &Context,
) -> Result<std::collections::BTreeSet<Event>> {
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
    wait_for_dm(ctx, Some(trade_keys), sent).await
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

/// The hash a buyer declares for `canonical` on `order_id`: the reusable
/// one that builds history, or, in full-privacy mode, the order-bound one
/// that keeps the node from linking the buyer's trades (protocol book,
/// "Full-privacy buyers").
pub(crate) fn declaration_hash(order_id: &Uuid, canonical: &str, full_privacy: bool) -> String {
    if full_privacy {
        order_bound_payment_hash(order_id, canonical)
    } else {
        payment_hash(canonical)
    }
}

/// Hash the details the buyer sent the seller and compare them with the
/// hash it declared to Mostro for `order_id`. Either construction matches:
/// both commit to the same details, and only the buyer knows which mode it
/// declared in.
pub(crate) fn check_declared(
    declared_hash: &str,
    order_id: &Uuid,
    method: &str,
    fields: &[String],
) -> Result<DeclarationCheck> {
    let canonical = canonical_payer(PayerMethod::parse(method)?, fields)?;
    let computed = payment_hash(&canonical);
    let matches = computed == declared_hash
        || order_bound_payment_hash(order_id, &canonical) == declared_hash;
    Ok(if matches {
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
    let trade_keys = trade_keys_of(&local_order(order_id, ctx).await?)?;
    // Full-privacy mode: `--secret` (Mostro then sees only the trade key),
    // or a context whose identity is the trade key itself.
    let full_privacy =
        parse_secret_env()? || ctx.identity_keys.public_key() == trade_keys.public_key();
    let hash = declaration_hash(order_id, &canonical, full_privacy);

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

    let payload = Payload::PayerDeclaration(PayerDeclaration::new(hash));
    let reply = request_until_answered(
        order_id,
        Action::DeclarePayer,
        Some(payload),
        false,
        &trade_keys,
        ctx,
    )
    .await?;
    print_commands_results(&reply, ctx).await
}

/// How many times a payer-history request is sent before giving up on a
/// reply.
const REQUEST_ATTEMPTS: usize = 3;

/// Send `action` and return Mostro's answer to it. `wait_for_dm` returns on
/// the first Mostro DM for the trade key, which can be an unrelated one (a
/// `fiat-sent-ok` landing at the same time), and a lost request or reply ends
/// in a wait timeout, so the request is re-sent until a batch holds the
/// answer. Every attempt carries the same request id, so
/// a reply to an earlier attempt that arrives late still matches. Re-sending
/// is safe for both callers: the history query only reads, and re-declaring
/// the same hash is a no-op.
/// `accept_history_push` also takes a same-order `payment-history` push as
/// the answer (it holds the same data as the query reply).
async fn request_until_answered(
    order_id: &Uuid,
    action: Action,
    payload: Option<Payload>,
    accept_history_push: bool,
    trade_keys: &Keys,
    ctx: &Context,
) -> Result<MessageKind> {
    let request_id = Uuid::new_v4().as_u128() as u64;
    for attempt in 1..=REQUEST_ATTEMPTS {
        let events = match send_to_mostro(
            order_id,
            request_id,
            action.clone(),
            payload.clone(),
            trade_keys,
            ctx,
        )
        .await
        {
            Ok(events) => events,
            // The request or its reply was lost: re-send. Any other failure
            // (PoW refused, relay error) would fail the same way again.
            Err(e) if e.downcast_ref::<WaitForDmTimeout>().is_some() => {
                if attempt == REQUEST_ATTEMPTS {
                    return Err(e);
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        let messages: Vec<Message> = parse_dm_events(events, trade_keys, None, true)
            .await
            .into_iter()
            .map(|(message, _, _)| message)
            .collect();
        let reply = if accept_history_push {
            pick_reply(&messages, request_id, order_id)
        } else {
            messages
                .iter()
                .map(|m| m.get_inner_message_kind())
                .find(|k| k.request_id == Some(request_id))
                .cloned()
        };
        if let Some(reply) = reply {
            return Ok(reply);
        }
    }
    Err(anyhow::anyhow!("No {action} reply received from Mostro"))
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

    let trade_keys = trade_keys_of(&local_order(order_id, ctx).await?)?;
    let reply = request_until_answered(
        order_id,
        Action::PaymentHistory,
        None,
        true,
        &trade_keys,
        ctx,
    )
    .await?;

    match reply.payload {
        Some(Payload::PaymentHistory(history)) => {
            let thresholds = fetch_payer_history_thresholds(ctx).await;
            println!("{}", format_payment_history(&history, thresholds));
            if let Some(method) = method {
                println!();
                match check_declared(&history.payment_hash, order_id, method, fields)? {
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
            // Mostro answers the same for "never declared" and "past success"
            // (the declaration was consumed), and the local status cache does
            // not track success, so do not claim either. No history was
            // retrieved whatever the cause, so fail for scripts.
            print_section_header("⚠️ No payer declaration available");
            println!(
                "💡 The buyer did not declare a payment sender, or the order is past success."
            );
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
        let order = Uuid::new_v4();
        let same = check_declared(
            &declared,
            &order,
            "EU|SEPA",
            &["de89 3704 0044 0532 0130 00".into(), "alice  smith".into()],
        )
        .unwrap();
        assert_eq!(same, DeclarationCheck::Match);

        let other = check_declared(
            &declared,
            &order,
            "EU|SEPA",
            &["DE44500105175407324931".into(), "Mallory Doe".into()],
        )
        .unwrap();
        assert!(matches!(other, DeclarationCheck::Mismatch { .. }));
    }

    #[test]
    fn full_privacy_declarations_are_bound_to_the_order() {
        let canonical = "EU|SEPA|DE89370400440532013000|ALICE SMITH";
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(
            declaration_hash(&a, canonical, false),
            payment_hash(canonical)
        );
        assert_ne!(
            declaration_hash(&a, canonical, true),
            declaration_hash(&b, canonical, true),
            "unlinkable across orders"
        );

        // The seller's check accepts the order-bound form for its own order
        // only.
        let fields = [
            "DE89370400440532013000".to_string(),
            "Alice Smith".to_string(),
        ];
        let bound = declaration_hash(&a, canonical, true);
        assert_eq!(
            check_declared(&bound, &a, "EU|SEPA", &fields).unwrap(),
            DeclarationCheck::Match
        );
        assert!(matches!(
            check_declared(&bound, &b, "EU|SEPA", &fields).unwrap(),
            DeclarationCheck::Mismatch { .. }
        ));
    }

    #[test]
    fn stdin_fields_are_one_per_line() {
        let input = "DE89 3704 0044 0532 0130 00\n\nAlice Smith\n";
        assert_eq!(
            read_fields(input.as_bytes()).unwrap(),
            vec![
                "DE89 3704 0044 0532 0130 00".to_string(),
                "Alice Smith".to_string()
            ]
        );
        assert_eq!(
            resolve_payer_fields(&["x".to_string()], false).unwrap(),
            vec!["x".to_string()]
        );
    }
}
