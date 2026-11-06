//! `order.rate` input and data: the rating the user gives the counterparty of
//! a finished trade and the node's acknowledgement. The acknowledgement must
//! carry the rating that was sent.

use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use uuid::Uuid;

use super::schema;

/// Canonical name of `order rate`.
pub const COMMAND: &str = "order.rate";

/// What the node answers a rating with, besides `cant-do`.
pub const REPLIES: [Action; 1] = [Action::RateReceived];

const RATINGS: std::ops::RangeInclusive<u8> = 1..=5;

/// The rating is outside 1 to 5. The value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidRating;

/// A validated rating of an order's counterparty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateInput {
    order: Uuid,
    rating: u8,
}

impl RateInput {
    pub fn validate(order: Uuid, rating: u8) -> Result<Self, InvalidRating> {
        if RATINGS.contains(&rating) {
            Ok(Self { order, rating })
        } else {
            Err(InvalidRating)
        }
    }

    pub fn order(&self) -> Uuid {
        self.order
    }

    pub fn rating(&self) -> u8 {
        self.rating
    }

    /// The payload of the request.
    pub fn payload(&self) -> Payload {
        Payload::RatingUser(self.rating)
    }
}

fn rating_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "minimum": 1, "maximum": 5})
}

/// The rating the node received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RateData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    /// The rating, from 1 to 5, as the node acknowledged it.
    #[schemars(schema_with = "rating_schema")]
    rating: u8,
    /// The public key of the order's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl RateData {
    /// The data of `rate-received` carrying the rating of `input`; `None` for
    /// any other reply.
    pub fn from_reply(
        node: &PublicKey,
        input: &RateInput,
        reply: (&Action, &Option<Payload>),
        trade_pubkey: &PublicKey,
    ) -> Option<Self> {
        match reply {
            (Action::RateReceived, Some(Payload::RatingUser(rating)))
                if *rating == input.rating =>
            {
                Some(Self {
                    node_pubkey: node.to_hex(),
                    order_id: input.order.to_string(),
                    rating: *rating,
                    trade_pubkey: trade_pubkey.to_hex(),
                })
            }
            _ => None,
        }
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn rating(&self) -> u8 {
        self.rating
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
    fn only_one_to_five_is_a_rating() {
        let order = Uuid::from_u128(1);
        for rating in 1..=5 {
            assert_eq!(RateInput::validate(order, rating).unwrap().rating(), rating);
        }
        for rating in [0, 6, 255] {
            assert_eq!(RateInput::validate(order, rating), Err(InvalidRating));
        }
    }

    #[test]
    fn the_acknowledgement_must_carry_the_rating_sent() {
        let (node, trade) = (Keys::generate().public_key(), Keys::generate().public_key());
        let input = RateInput::validate(Uuid::from_u128(1), 4).unwrap();
        let ok = RateData::from_reply(
            &node,
            &input,
            (&Action::RateReceived, &Some(Payload::RatingUser(4))),
            &trade,
        );
        assert_eq!(ok.unwrap().rating(), 4);
        for (action, payload) in [
            (Action::RateReceived, Some(Payload::RatingUser(5))),
            (Action::RateReceived, None),
            (Action::Rate, Some(Payload::RatingUser(4))),
        ] {
            assert!(
                RateData::from_reply(&node, &input, (&action, &payload), &trade).is_none(),
                "{action:?} {payload:?}"
            );
        }
    }
}
