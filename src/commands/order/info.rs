//! `order.info`: ask the node for its record of the user's own orders.
//!
//! One signed request goes out through `msg::exchange` and the reply is mapped
//! to a typed outcome. The command reads no storage, prints nothing and never
//! touches a client; the identity key is an argument.

use mostro_core::prelude::{Action, CantDoReason, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use crate::msg::{exchange, ExchangeFailure, ExchangeLimits, Expect, Outbound, PublicReadConfig};
use crate::outcome::info::{OrderInfoData, OrderInfoInput};
use crate::outcome::{DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Publication, Warnings};

pub const COMMAND: &str = "order.info";

pub type InfoOutcome = Envelope<OrderInfoData, ()>;

/// How the request is sent: the work to mine and whether to check the node's
/// protocol (off when the user chose the transport explicitly).
#[derive(Clone, Copy, Debug)]
pub struct InfoOptions {
    pub pow: u8,
    pub check_protocol: bool,
}

/// Ask the node about `requested`, signing with `identity`.
pub async fn order_info(
    requested: &OrderInfoInput,
    config: &PublicReadConfig,
    identity: &Keys,
    options: InfoOptions,
    limits: &ExchangeLimits,
) -> InfoOutcome {
    let request = Message::new_order(
        None,
        Some(Uuid::new_v4().as_u128() as u64),
        None,
        Action::Orders,
        Some(Payload::Ids(requested.ids().to_vec())),
    );
    let outbound = Outbound {
        identity,
        signer: identity,
        recipient: config.mostro_pubkey(),
        message: request,
        pow: options.pow,
        check_protocol: options.check_protocol,
    };
    let expect = Expect {
        allowed_actions: vec![Action::Orders, Action::CantDo],
    };
    match exchange(config, outbound, &expect, limits).await {
        Err(failure) => failed(&failure),
        Ok(exchanged) => {
            let mut outcome = answer(config, requested, exchanged.reply());
            outcome.add_warnings(exchanged.warnings());
            outcome
        }
    }
}

fn failed(failure: &ExchangeFailure) -> InfoOutcome {
    Envelope::error(
        Some(COMMAND),
        failure.code(),
        failure.warnings().clone(),
        ExecutionMeta::request(failure.publication(), failure.daemon_acceptance()),
    )
}

/// The outcome for the node's reply. Only `not_found` is turned into a
/// success: it means none of the ids is the caller's, the same fact as a
/// reply that leaves them out. Every other refusal is an error.
fn answer(config: &PublicReadConfig, requested: &OrderInfoInput, reply: &Message) -> InfoOutcome {
    let kind = reply.get_inner_message_kind();
    let node = config.mostro_pubkey();
    let delivered = |daemon| ExecutionMeta::request(Publication::Published, daemon);
    match (&kind.action, &kind.payload) {
        (Action::Orders, Some(Payload::Orders(orders))) => {
            let (data, warnings) = OrderInfoData::from_reply(&node, requested, orders);
            Envelope::ok(
                COMMAND,
                data,
                warnings,
                delivered(DaemonAcceptance::Accepted),
            )
        }
        (Action::CantDo, Some(Payload::CantDo(Some(CantDoReason::NotFound)))) => {
            let (data, warnings) = OrderInfoData::from_reply(&node, requested, &[]);
            Envelope::ok(
                COMMAND,
                data,
                warnings,
                delivered(DaemonAcceptance::Accepted),
            )
        }
        (Action::CantDo, payload) => {
            let reason = match payload {
                Some(Payload::CantDo(Some(reason))) => serde_json::to_value(reason)
                    .ok()
                    .and_then(|name| name.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            Envelope::refused(
                Some(COMMAND),
                &reason,
                Warnings::new(),
                delivered(DaemonAcceptance::Rejected),
            )
        }
        _ => Envelope::error(
            Some(COMMAND),
            ErrorCode::ReplyMalformed,
            Warnings::new(),
            delivered(DaemonAcceptance::Unknown),
        ),
    }
}
