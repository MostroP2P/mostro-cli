//! `admin.order-info`: ask the node for its snapshot of one of its orders.
//!
//! One request signed with the admin key goes out through `msg::exchange` and
//! the reply is mapped to a typed outcome. The command reads no storage and no
//! environment, prints nothing and never touches a client; the key is an
//! argument.

use mostro_core::prelude::{Action, CantDoReason, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use crate::commands::order::info::InfoOptions;
use crate::msg::{
    exchange, ExchangeFailure, ExchangeLimits, Expect, Lane, Outbound, PublicReadConfig,
};
use crate::outcome::admin_order_info::{AdminOrderInfoData, COMMAND};
use crate::outcome::{DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Publication, Warnings};

pub type AdminOrderInfoOutcome = Envelope<AdminOrderInfoData, ()>;

/// Ask the node about `order`, signing with `admin`: the key of the node's
/// admin or of one of its registered solvers, used as identity and author.
pub async fn admin_order_info(
    order: Uuid,
    config: &PublicReadConfig,
    admin: &Keys,
    options: InfoOptions,
    limits: &ExchangeLimits,
) -> AdminOrderInfoOutcome {
    let request = Message::new_order(
        Some(order),
        Some(Uuid::new_v4().as_u128() as u64),
        None,
        Action::AdminOrderInfo,
        None,
    );
    let outbound = Outbound {
        identity: admin,
        signer: admin,
        recipient: config.mostro_pubkey(),
        message: request,
        pow: options.pow,
        // The node knows a solver's key only while it holds an active
        // dispute; work mined for the first-contact lane is accepted on both.
        lane: Lane::FirstContact,
        check_protocol: options.check_protocol,
    };
    let expect = Expect {
        allowed_actions: vec![Action::AdminOrderInfo, Action::CantDo],
        by_order: None,
    };
    match exchange(config, outbound, &expect, limits).await {
        Err(failure) => failed(&failure),
        Ok(exchanged) => {
            let mut outcome = answer(config, order, exchanged.reply());
            outcome.add_warnings(exchanged.warnings());
            outcome
        }
    }
}

fn failed(failure: &ExchangeFailure) -> AdminOrderInfoOutcome {
    Envelope::error(
        Some(COMMAND),
        failure.code(),
        failure.warnings().clone(),
        ExecutionMeta::request(failure.publication(), failure.daemon_acceptance()),
    )
    .with_work(failure.work())
}

/// The outcome for the node's reply. A snapshot is accepted only for the
/// order that was asked about. Every refusal is an error with the node's
/// reason: `not_found` is not an empty success here, one order was asked for.
fn answer(config: &PublicReadConfig, order: Uuid, reply: &Message) -> AdminOrderInfoOutcome {
    let kind = reply.get_inner_message_kind();
    let delivered = |daemon| ExecutionMeta::request(Publication::Published, daemon);
    match (&kind.action, &kind.payload) {
        (Action::AdminOrderInfo, Some(Payload::OrderInfo(info)))
            if kind.id == Some(order) && info.id == order =>
        {
            let (data, warnings) = AdminOrderInfoData::from_reply(&config.mostro_pubkey(), info);
            Envelope::ok(
                COMMAND,
                data,
                warnings,
                delivered(DaemonAcceptance::Accepted),
            )
        }
        (Action::CantDo, payload) => Envelope::refused(
            Some(COMMAND),
            &reason(payload.as_ref()),
            Warnings::new(),
            delivered(DaemonAcceptance::Rejected),
        ),
        _ => Envelope::error(
            Some(COMMAND),
            ErrorCode::ReplyMalformed,
            Warnings::new(),
            delivered(DaemonAcceptance::Unknown),
        ),
    }
}

/// The node's refusal reason in its wire spelling; empty when it gave none.
fn reason(payload: Option<&Payload>) -> String {
    match payload {
        Some(Payload::CantDo(Some(reason))) => wire_name(reason),
        _ => String::new(),
    }
}

fn wire_name(reason: &CantDoReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::OrderInfo;
    use serde_json::{json, Value};

    const ORDER: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);

    fn config() -> PublicReadConfig {
        let node = Keys::generate().public_key().to_hex();
        PublicReadConfig::resolve(Some(&node), Some("ws://127.0.0.1:1"), |_| None).unwrap()
    }

    fn snapshot(id: Uuid) -> Payload {
        Payload::OrderInfo(OrderInfo {
            id,
            kind: "sell".to_string(),
            status: "active".to_string(),
            ..Default::default()
        })
    }

    fn reply(id: Option<Uuid>, action: Action, payload: Option<Payload>) -> Message {
        Message::new_order(id, Some(7), None, action, payload)
    }

    fn outcome(reply: &Message) -> Value {
        serde_json::to_value(answer(&config(), ORDER, reply)).unwrap()
    }

    #[test]
    fn a_snapshot_of_the_requested_order_is_ok() {
        let outcome = outcome(&reply(
            Some(ORDER),
            Action::AdminOrderInfo,
            Some(snapshot(ORDER)),
        ));

        assert_eq!(outcome["command"], "admin.order-info");
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["data"]["order_id"], ORDER.to_string());
        assert_eq!(outcome["data"]["status"], "active");
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "not_applicable", "snapshot": null})
        );
    }

    /// A snapshot is taken only for the order that was asked about, whether
    /// the message or the payload names another one.
    #[test]
    fn a_snapshot_of_another_order_is_reply_malformed() {
        for reply in [
            reply(Some(ORDER), Action::AdminOrderInfo, Some(snapshot(OTHER))),
            reply(Some(OTHER), Action::AdminOrderInfo, Some(snapshot(ORDER))),
            reply(None, Action::AdminOrderInfo, Some(snapshot(ORDER))),
        ] {
            let outcome = outcome(&reply);
            assert_eq!(outcome["error"]["code"], "reply_malformed", "{reply:?}");
            assert_eq!(outcome["data"], Value::Null);
            assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
        }
    }

    #[test]
    fn a_matching_reply_without_a_snapshot_is_reply_malformed() {
        for payload in [None, Some(Payload::TextMessage("no".to_string()))] {
            let outcome = outcome(&reply(Some(ORDER), Action::AdminOrderInfo, payload));
            assert_eq!(outcome["error"]["code"], "reply_malformed");
        }
    }

    /// `not_found` is a refusal here, not the empty success of `order.info`.
    #[test]
    fn a_refusal_is_node_refused_with_the_reason_of_the_node() {
        for (reason, name) in [
            (CantDoReason::InvalidPubkey, "invalid_pubkey"),
            (CantDoReason::NotFound, "not_found"),
        ] {
            let refusal =
                Message::cant_do(Some(ORDER), Some(7), Some(Payload::CantDo(Some(reason))));
            let outcome = outcome(&refusal);
            assert_eq!(outcome["status"], "error");
            assert_eq!(outcome["error"]["code"], "node_refused");
            assert_eq!(outcome["error"]["reason"], name);
            assert_eq!(outcome["data"], Value::Null);
            assert_eq!(outcome["meta"]["daemon_acceptance"], "rejected");
        }
    }

    #[test]
    fn a_refusal_without_a_reason_is_still_node_refused() {
        let outcome = outcome(&Message::cant_do(Some(ORDER), Some(7), None));
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert!(outcome["error"].get("reason").is_none());
    }
}
