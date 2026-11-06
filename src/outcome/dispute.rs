//! `dispute.open` data: the dispute the node opened on an order the user
//! holds. The node's reply is untrusted: it must name the order and carry a
//! dispute payload; nothing else of it is kept.

use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::schema;

/// Canonical name of `dispute open`.
pub const COMMAND: &str = "dispute.open";

/// What the node answers a dispute with, besides `cant-do`.
pub const REPLIES: [Action; 1] = [Action::DisputeInitiatedByYou];

/// The dispute the node opened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DisputeOpenData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    /// The id of the dispute the node created.
    #[schemars(schema_with = "schema::uuid")]
    dispute_id: String,
    /// The public key of the order's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl DisputeOpenData {
    /// The data of `dispute-initiated-by-you` with its dispute payload; `None`
    /// for any other reply.
    pub fn from_reply(
        node: &PublicKey,
        order: Uuid,
        reply: (&Action, &Option<Payload>),
        trade_pubkey: &PublicKey,
    ) -> Option<Self> {
        match reply {
            (Action::DisputeInitiatedByYou, Some(Payload::Dispute(dispute, _))) => Some(Self {
                node_pubkey: node.to_hex(),
                order_id: order.to_string(),
                dispute_id: dispute.to_string(),
                trade_pubkey: trade_pubkey.to_hex(),
            }),
            _ => None,
        }
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn dispute_id(&self) -> &str {
        &self.dispute_id
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;

    #[test]
    fn only_the_initiators_notice_with_a_dispute_id_is_used() {
        let (node, trade) = (Keys::generate().public_key(), Keys::generate().public_key());
        let (order, dispute) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let data = DisputeOpenData::from_reply(
            &node,
            order,
            (
                &Action::DisputeInitiatedByYou,
                &Some(Payload::Dispute(dispute, None)),
            ),
            &trade,
        )
        .unwrap();
        assert_eq!(data.dispute_id(), dispute.to_string());
        assert_eq!(data.order_id(), order.to_string());
        for (action, payload) in [
            (Action::DisputeInitiatedByYou, None),
            (
                Action::DisputeInitiatedByPeer,
                Some(Payload::Dispute(dispute, None)),
            ),
            (
                Action::DisputeInitiatedByYou,
                Some(Payload::TextMessage("x".into())),
            ),
        ] {
            assert!(
                DisputeOpenData::from_reply(&node, order, (&action, &payload), &trade).is_none(),
                "{action:?}"
            );
        }
    }
}
