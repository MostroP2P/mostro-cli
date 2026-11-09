//! The counterparty of an order, learned from the node's own messages.
//!
//! The peer chat of an order is encrypted to the counterparty's trade key, and
//! only the node can vouch for that key: chat messages are opened with our own
//! trade key, so a counterparty could name any key in one. So the key is taken
//! from the node's mail only ([`Mail`] holds what `inbox::open_events`
//! accepted: authored and sealed by the node, addressed to our trade key), and
//! only from the messages the node sends once the escrow is locked:
//!
//! - `buyer-took-order` (to the seller) and `hold-invoice-payment-accepted`
//!   (to the buyer), whose `Order` payload names both trade keys;
//! - `fiat-sent-ok`, whose `Peer` payload names the other side.
//!
//! A message sent before the lock (`waiting-seller-to-pay`, a `new-order`
//! republication), the solver named by `admin-took-dispute` and any message
//! about another order never name the counterparty.

use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use uuid::Uuid;

use super::inbox::Mail;

/// The counterparty of `order` named by the newest post-lock message of the
/// node addressed to `ours`, or `None` when no such message names one.
pub fn counterparty_in(mail: &[Mail], order: Uuid, ours: &PublicKey) -> Option<PublicKey> {
    mail.iter()
        .filter(|mail| mail.key == *ours)
        .filter_map(|mail| Some(((mail.created_at, mail.event), named_in(mail, order, ours)?)))
        .max_by_key(|(newest, _)| *newest)
        .map(|(_, key)| key)
}

/// The key that is not `ours` in one post-lock message about `order`.
fn named_in(mail: &Mail, order: Uuid, ours: &PublicKey) -> Option<PublicKey> {
    let kind = mail.message.get_inner_message_kind();
    let key = match (&kind.action, kind.payload.as_ref()?) {
        (Action::BuyerTookOrder | Action::HoldInvoicePaymentAccepted, Payload::Order(small))
            if small.id.or(kind.id) == Some(order) =>
        {
            let buyer = small.buyer_trade_pubkey.as_deref()?;
            let seller = small.seller_trade_pubkey.as_deref()?;
            counterparty_from_trade_pubkeys(ours, buyer, seller)?
        }
        (Action::FiatSentOk, Payload::Peer(peer)) if kind.id == Some(order) => {
            PublicKey::parse(&peer.pubkey).ok()?
        }
        _ => return None,
    };
    (key != *ours).then_some(key)
}

/// Pick the trade pubkey that is not `ours` from a buyer/seller pair.
///
/// Both sides are parsed as [`PublicKey`] so hex and npub compare equal.
pub fn counterparty_from_trade_pubkeys(
    ours: &PublicKey,
    buyer: &str,
    seller: &str,
) -> Option<PublicKey> {
    let buyer = PublicKey::parse(buyer).ok()?;
    let seller = PublicKey::parse(seller).ok()?;
    if buyer == *ours {
        Some(seller)
    } else if seller == *ours {
        Some(buyer)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use mostro_core::prelude::{
        wrap_message_with, Message, Peer, SmallOrder, Transport, WrapOptions,
    };
    use nostr_sdk::prelude::{
        Event, EventBuilder, EventId, FinalizeEvent, Keys, Kind, Timestamp, ToBech32,
    };

    use super::*;
    use crate::msg::inbox::open_events;

    const ORDER: Uuid = Uuid::from_u128(0x0c4a7);
    const OTHER: Uuid = Uuid::from_u128(0x07e4);

    /// An `Order` payload naming `buyer` and `seller`, with the order id in
    /// the payload when `in_payload`, otherwise only in the message.
    fn locked(
        action: Action,
        order: Uuid,
        in_payload: bool,
        (buyer, seller): (&PublicKey, &PublicKey),
    ) -> Message {
        let small = SmallOrder {
            id: in_payload.then_some(order),
            buyer_trade_pubkey: Some(buyer.to_hex()),
            seller_trade_pubkey: Some(seller.to_hex()),
            ..SmallOrder::default()
        };
        Message::new_order(Some(order), None, None, action, Some(Payload::Order(small)))
    }

    fn peer(action: Action, order: Uuid, pubkey: &str) -> Message {
        let peer = Peer {
            pubkey: pubkey.to_string(),
            reputation: None,
        };
        Message::new_order(Some(order), None, None, action, Some(Payload::Peer(peer)))
    }

    /// The node's message addressed to `ours`, opened, at `at`.
    fn mail(at: u64, message: Message, ours: &PublicKey) -> Mail {
        Mail {
            event: EventId::from_byte_array([u8::try_from(at % 256).unwrap(); 32]),
            created_at: at,
            message,
            index: None,
            key: *ours,
        }
    }

    #[test]
    fn counterparty_from_trade_pubkeys_picks_the_other_side() {
        let ours = Keys::generate().public_key();
        let them = Keys::generate().public_key();
        assert_eq!(
            counterparty_from_trade_pubkeys(&ours, &ours.to_hex(), &them.to_hex()),
            Some(them)
        );
        assert_eq!(
            counterparty_from_trade_pubkeys(&ours, &them.to_hex(), &ours.to_hex()),
            Some(them)
        );
    }

    #[test]
    fn counterparty_from_trade_pubkeys_accepts_npub() {
        let ours = Keys::generate().public_key();
        let them = Keys::generate().public_key();
        let ours_npub = ours.to_bech32().expect("npub");
        let them_hex = them.to_hex();
        assert_eq!(
            counterparty_from_trade_pubkeys(&ours, &ours_npub, &them_hex),
            Some(them)
        );
    }

    #[test]
    fn counterparty_from_trade_pubkeys_returns_none_when_neither_is_ours() {
        let ours = Keys::generate().public_key();
        let buyer = Keys::generate().public_key().to_hex();
        let seller = Keys::generate().public_key().to_hex();
        assert!(counterparty_from_trade_pubkeys(&ours, &buyer, &seller).is_none());
    }

    #[test]
    fn lock_messages_name_the_other_trade_key() {
        let (ours, them) = (Keys::generate().public_key(), Keys::generate().public_key());
        for (action, in_payload, pair) in [
            (Action::BuyerTookOrder, true, (&them, &ours)),
            (Action::BuyerTookOrder, false, (&them, &ours)),
            (Action::HoldInvoicePaymentAccepted, true, (&ours, &them)),
            (Action::HoldInvoicePaymentAccepted, false, (&ours, &them)),
        ] {
            let found = counterparty_in(
                &[mail(
                    10,
                    locked(action.clone(), ORDER, in_payload, pair),
                    &ours,
                )],
                ORDER,
                &ours,
            );
            assert_eq!(found, Some(them), "{action:?}, id in payload: {in_payload}");
        }
    }

    #[test]
    fn fiat_sent_ok_names_the_counterparty_in_its_peer_payload() {
        let (ours, them) = (Keys::generate().public_key(), Keys::generate().public_key());
        for pubkey in [them.to_hex(), them.to_bech32().unwrap()] {
            let message = peer(Action::FiatSentOk, ORDER, &pubkey);
            assert_eq!(
                counterparty_in(&[mail(10, message, &ours)], ORDER, &ours),
                Some(them)
            );
        }
    }

    #[test]
    fn only_post_lock_messages_about_this_order_count() {
        let (ours, them, stranger) = (
            Keys::generate().public_key(),
            Keys::generate().public_key(),
            Keys::generate().public_key(),
        );
        let solver = Keys::generate().public_key();
        let cases = [
            (
                "waiting-seller-to-pay naming both keys",
                locked(Action::WaitingSellerToPay, ORDER, true, (&ours, &them)),
            ),
            (
                "a new-order republication",
                locked(Action::NewOrder, ORDER, true, (&ours, &them)),
            ),
            (
                "the solver of admin-took-dispute",
                peer(Action::AdminTookDispute, ORDER, &solver.to_hex()),
            ),
            (
                "another order",
                locked(Action::BuyerTookOrder, OTHER, true, (&them, &ours)),
            ),
            (
                "another order, fiat-sent-ok",
                peer(Action::FiatSentOk, OTHER, &them.to_hex()),
            ),
            (
                "neither key is ours",
                locked(Action::BuyerTookOrder, ORDER, true, (&them, &stranger)),
            ),
            (
                "both keys are ours",
                locked(Action::BuyerTookOrder, ORDER, true, (&ours, &ours)),
            ),
            (
                "a peer that is ours",
                peer(Action::FiatSentOk, ORDER, &ours.to_hex()),
            ),
            (
                "an unparsable peer",
                peer(Action::FiatSentOk, ORDER, "not-a-key"),
            ),
        ];
        for (label, message) in cases {
            assert_eq!(
                counterparty_in(&[mail(10, message, &ours)], ORDER, &ours),
                None,
                "{label}"
            );
        }
        let elsewhere = mail(
            10,
            peer(Action::FiatSentOk, ORDER, &them.to_hex()),
            &stranger,
        );
        assert_eq!(
            counterparty_in(&[elsewhere], ORDER, &ours),
            None,
            "mail addressed to another key"
        );
    }

    #[test]
    fn the_newest_accepted_message_wins() {
        let (ours, old, new) = (
            Keys::generate().public_key(),
            Keys::generate().public_key(),
            Keys::generate().public_key(),
        );
        let mail = [
            mail(30, peer(Action::FiatSentOk, ORDER, &new.to_hex()), &ours),
            mail(
                10,
                locked(Action::BuyerTookOrder, ORDER, true, (&old, &ours)),
                &ours,
            ),
            mail(
                40,
                locked(Action::WaitingSellerToPay, ORDER, true, (&old, &ours)),
                &ours,
            ),
        ];
        assert_eq!(counterparty_in(&mail, ORDER, &ours), Some(new));
    }

    async fn sealed(author: &Keys, to: &Keys, message: &Message) -> Event {
        let wrapped = wrap_message_with(
            Transport::Nip44Direct,
            message,
            author,
            author,
            to.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();
        EventBuilder::new(Kind::PrivateDirectMessage, wrapped.content.clone())
            .tags(wrapped.tags.clone())
            .custom_created_at(Timestamp::from(1_000))
            .finalize(author)
            .unwrap()
    }

    #[tokio::test]
    async fn an_impostor_event_never_names_the_counterparty() {
        let (node, impostor, ours) = (Keys::generate(), Keys::generate(), Keys::generate());
        let them = Keys::generate().public_key();
        let message = locked(
            Action::BuyerTookOrder,
            ORDER,
            true,
            (&them, &ours.public_key()),
        );
        let (keys, node_pubkey) = ([(None, ours.clone())], node.public_key());
        let open = |event: Event| {
            let keys = keys.clone();
            let events: BTreeSet<Event> = [event].into_iter().collect();
            async move { open_events(&events, &keys, node_pubkey, Timestamp::from(0)).await }
        };

        let forged = open(sealed(&impostor, &ours, &message).await).await;
        let genuine = open(sealed(&node, &ours, &message).await).await;

        assert_eq!(
            counterparty_in(&forged, ORDER, &ours.public_key()),
            None,
            "an Order payload another key authored"
        );
        assert_eq!(
            counterparty_in(&genuine, ORDER, &ours.public_key()),
            Some(them)
        );
    }
}
