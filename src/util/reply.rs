//! Reply correlation: decide whether an incoming kind-14 is Mostro's answer to
//! the request this client sent.

use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::parser::parse_dm_events;

/// Whether `event` is a candidate reply from Mostro: a kind-14 authored by
/// Mostro, tagged to `trade_pubkey`, and not the request we sent ourselves.
fn is_mostro_reply(
    event: &Event,
    trade_pubkey: PublicKey,
    mostro_pubkey: PublicKey,
    sent_event_id: Option<EventId>,
) -> bool {
    event.kind == nostr_sdk::prelude::Kind::PrivateDirectMessage
        && event.pubkey == mostro_pubkey
        && event.tags.public_keys().any(|pk| pk == trade_pubkey)
        && Some(event.id) != sent_event_id
}

/// Correlation criteria for [`wait_for_dm_matching`]: exact `request_id`, and
/// optionally an allow-list of actions (empty = any action).
#[derive(Clone, Debug)]
pub struct ExpectedReply {
    request_id: u64,
    allowed_actions: Vec<Action>,
    by_order: Option<(Uuid, Vec<Action>)>,
}

impl ExpectedReply {
    /// Build correlation criteria that match `request_id` and, when
    /// `allowed_actions` is non-empty, one of those actions.
    pub fn new(request_id: u64, allowed_actions: Vec<Action>) -> Self {
        Self {
            request_id,
            allowed_actions,
            by_order: None,
        }
    }

    /// Also accept a message that carries **no** request id but names `order`
    /// and one of `actions`. For acknowledgements the node sends without
    /// echoing the request (`bond-invoice-accepted`); a message that carries a
    /// different request id is never accepted this way.
    pub fn or_by_order(mut self, order: Uuid, actions: Vec<Action>) -> Self {
        self.by_order = Some((order, actions));
        self
    }
}

/// True when `message` echoes `expected.request_id` and (if the allow-list is
/// non-empty) carries an allowed action. Checks the id **before** any
/// CantDo-specific handling so an uncorrelated `CantDo` is never accepted.
fn message_matches_expected_reply(message: &Message, expected: &ExpectedReply) -> bool {
    let inner = message.get_inner_message_kind();
    if inner.request_id.is_none() {
        return expected.by_order.as_ref().is_some_and(|(order, actions)| {
            inner.id == Some(*order) && actions.contains(&inner.action)
        });
    }
    if inner.request_id != Some(expected.request_id) {
        return false;
    }
    expected.allowed_actions.is_empty() || expected.allowed_actions.contains(&inner.action)
}

async fn event_matches_expected_reply(
    event: &Event,
    trade_keys: &Keys,
    expected: &ExpectedReply,
) -> bool {
    let mut one = BTreeSet::new();
    one.insert(event.clone());
    let messages = parse_dm_events(one, trade_keys, None, true).await;
    messages
        .first()
        .is_some_and(|(msg, _, _)| message_matches_expected_reply(msg, expected))
}

/// Decide whether `event` is the reply [`wait_for_reply`] should return.
///
/// Accepts only a Mostro-authored kind-14 tagged to our trade key that is not
/// the echo of our own request ([`is_mostro_reply`]). When `expected` is set,
/// the event's decrypted message must also echo the request_id and an allowed
/// action. Any unrelated kind-14 — NIP-17 peer chat, our own echo, or a stale
/// push with a different id/action arriving before the real reply — returns
/// `false`, so the caller keeps waiting instead of racing on the first kind-14.
pub async fn select_reply_event(
    event: &Event,
    trade_keys: &Keys,
    mostro_pubkey: PublicKey,
    sent_event_id: Option<EventId>,
    expected: Option<&ExpectedReply>,
) -> bool {
    if !is_mostro_reply(event, trade_keys.public_key(), mostro_pubkey, sent_event_id) {
        return false;
    }
    match expected {
        Some(expected) => event_matches_expected_reply(event, trade_keys, expected).await,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind14_event(author: &Keys, tagged: PublicKey) -> Event {
        kind14_event_with(author, tagged, "ciphertext")
    }

    fn kind14_event_with(author: &Keys, tagged: PublicKey, content: &str) -> Event {
        EventBuilder::new(nostr_sdk::prelude::Kind::PrivateDirectMessage, content)
            .tag(Tag::public_key(tagged))
            .finalize_unsigned(author.public_key())
            .finalize(author)
            .unwrap()
    }

    #[test]
    fn is_mostro_reply_accepts_kind14_from_mostro_tagged_to_us() {
        let mostro = Keys::generate();
        let me = Keys::generate();
        let event = kind14_event(&mostro, me.public_key());
        assert!(is_mostro_reply(
            &event,
            me.public_key(),
            mostro.public_key(),
            None
        ));
    }

    #[test]
    fn is_mostro_reply_rejects_other_author_kind_or_tag() {
        let mostro = Keys::generate();
        let me = Keys::generate();
        let stranger = Keys::generate();
        let from_stranger = kind14_event(&stranger, me.public_key());
        let tagged_elsewhere = kind14_event(&mostro, stranger.public_key());
        let wrong_kind = EventBuilder::new(nostr_sdk::prelude::Kind::TextNote, "x")
            .tag(Tag::public_key(me.public_key()))
            .finalize_unsigned(mostro.public_key())
            .finalize(&mostro)
            .unwrap();
        for event in [from_stranger, tagged_elsewhere, wrong_kind] {
            assert!(!is_mostro_reply(
                &event,
                me.public_key(),
                mostro.public_key(),
                None
            ));
        }
    }

    #[test]
    fn is_mostro_reply_skips_our_own_request_when_admin_is_mostro() {
        // Admin flows sign with the Mostro key itself, so the outbound
        // request matches the reply filter; only its event id tells it apart.
        let mostro = Keys::generate();
        let request = kind14_event_with(&mostro, mostro.public_key(), "request");
        let reply = kind14_event_with(&mostro, mostro.public_key(), "reply");
        assert!(!is_mostro_reply(
            &request,
            mostro.public_key(),
            mostro.public_key(),
            Some(request.id)
        ));
        assert!(is_mostro_reply(
            &reply,
            mostro.public_key(),
            mostro.public_key(),
            Some(request.id)
        ));
    }

    fn user_info_expected(request_id: u64) -> ExpectedReply {
        ExpectedReply {
            request_id,
            allowed_actions: vec![Action::UserInfo, Action::CantDo],
            by_order: None,
        }
    }

    fn user_info_message(request_id: Option<u64>) -> Message {
        Message::Restore(MessageKind::new(
            None,
            request_id,
            None,
            Action::UserInfo,
            Some(Payload::UserInfo(UserInfo::default())),
        ))
    }

    fn cant_do_message(request_id: Option<u64>) -> Message {
        Message::CantDo(MessageKind::new(
            None,
            request_id,
            None,
            Action::CantDo,
            Some(Payload::CantDo(Some(CantDoReason::InvalidAction))),
        ))
    }

    fn rate_message(request_id: Option<u64>) -> Message {
        Message::Order(MessageKind::new(None, request_id, None, Action::Rate, None))
    }

    #[test]
    fn correlated_user_info_accepts_matching_id() {
        let expected = user_info_expected(42);
        assert!(message_matches_expected_reply(
            &user_info_message(Some(42)),
            &expected
        ));
    }

    #[test]
    fn correlated_user_info_rejects_mismatched_or_missing_id() {
        let expected = user_info_expected(42);
        assert!(!message_matches_expected_reply(
            &user_info_message(Some(7)),
            &expected
        ));
        assert!(!message_matches_expected_reply(
            &user_info_message(None),
            &expected
        ));
    }

    #[test]
    fn correlated_cant_do_accepted_only_with_matching_id() {
        let expected = user_info_expected(42);
        assert!(
            message_matches_expected_reply(&cant_do_message(Some(42)), &expected),
            "CantDo must be accepted when it echoes the request_id"
        );
        assert!(
            !message_matches_expected_reply(&cant_do_message(Some(7)), &expected),
            "uncorrelated CantDo must be skipped (validate_response would \
             short-circuit on CantDo before checking the id)"
        );
        assert!(!message_matches_expected_reply(
            &cant_do_message(None),
            &expected
        ));
    }

    #[test]
    fn unrelated_action_with_matching_id_is_skipped() {
        let expected = user_info_expected(42);
        assert!(
            !message_matches_expected_reply(&rate_message(Some(42)), &expected),
            "stale/concurrent reply with a different action must not win the wait"
        );
    }

    fn ack(request_id: Option<u64>, order: Option<Uuid>, action: Action) -> Message {
        Message::Order(MessageKind::new(order, request_id, None, action, None))
    }

    fn by_order_expected(order: Uuid) -> ExpectedReply {
        ExpectedReply::new(42, vec![Action::CantDo])
            .or_by_order(order, vec![Action::BondInvoiceAccepted])
    }

    #[test]
    fn an_ack_without_a_request_id_is_matched_on_its_order_and_action() {
        let order = Uuid::from_u128(5);
        let expected = by_order_expected(order);

        assert!(message_matches_expected_reply(
            &ack(None, Some(order), Action::BondInvoiceAccepted),
            &expected
        ));
        // The request id still matches as before.
        assert!(message_matches_expected_reply(
            &ack(Some(42), None, Action::CantDo),
            &expected
        ));
    }

    #[test]
    fn the_order_match_never_accepts_another_order_action_or_request_id() {
        let order = Uuid::from_u128(5);
        let expected = by_order_expected(order);

        for (label, message) in [
            (
                "another order",
                ack(None, Some(Uuid::from_u128(6)), Action::BondInvoiceAccepted),
            ),
            ("no order", ack(None, None, Action::BondInvoiceAccepted)),
            (
                "an action only the request id may carry",
                ack(None, Some(order), Action::CantDo),
            ),
            (
                "a stale request id",
                ack(Some(7), Some(order), Action::BondInvoiceAccepted),
            ),
        ] {
            assert!(
                !message_matches_expected_reply(&message, &expected),
                "{label}"
            );
        }
    }

    #[test]
    fn without_an_order_match_an_ack_with_no_request_id_is_never_accepted() {
        let order = Uuid::from_u128(5);
        let expected = ExpectedReply::new(42, vec![Action::BondInvoiceAccepted]);

        assert!(!message_matches_expected_reply(
            &ack(None, Some(order), Action::BondInvoiceAccepted),
            &expected
        ));
    }

    #[test]
    fn empty_allow_list_matches_any_action_with_request_id() {
        let expected = ExpectedReply {
            request_id: 42,
            allowed_actions: vec![],
            by_order: None,
        };
        assert!(message_matches_expected_reply(
            &rate_message(Some(42)),
            &expected
        ));
        assert!(!message_matches_expected_reply(
            &rate_message(Some(7)),
            &expected
        ));
    }
}
