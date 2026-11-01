//! Tests for `message list` (spec 006).
//!
//! Slice 1 holds the self-tests of the mailbox fixture (messages the node
//! volunteers to chosen trade keys) and the characterization of what `getdm`
//! does today. The command's own scenarios are added with the slices that
//! implement it.

use std::collections::BTreeSet;
use std::time::Duration;

use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{unwrap_incoming, Action, CantDoReason, Message, Payload};
use nostr_sdk::prelude::{
    Client, Event, Filter, FinalizeEvent, Keys, Kind, LocalRelay, PublicKey, Timestamp,
};
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    mail_add_invoice, mail_bond_payout_request, mail_cant_do, mail_pay_bond_invoice,
    mail_pay_invoice, mail_text, mail_waiting_seller, node_event,
};
use harness::{Backend, LoopbackRelay};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn order(n: u128, status: Status) -> SmallOrder {
    SmallOrder::new(
        Some(id(n)),
        Some(OrderKind::Sell),
        Some(status),
        6_233,
        "USD".to_string(),
        None,
        None,
        5,
        "bank".to_string(),
        1,
        None,
        None,
        None,
        Some(1_792_108_800),
        Some(1_792_195_200),
    )
}

fn minutes_ago(minutes: u64) -> Timestamp {
    Timestamp::from(Timestamp::now().as_secs() - minutes * 60)
}

// --- The mailbox fixture ------------------------------------------------------

mod mailbox {
    use super::*;

    #[tokio::test]
    async fn a_node_event_opens_with_the_addressed_key_and_with_no_other() {
        let (node, trade, stranger) = (Keys::generate(), Keys::generate(), Keys::generate());
        let message = mail_add_invoice(&order(1, Status::WaitingBuyerInvoice));
        let at = minutes_ago(90);

        let event = node_event(&node, trade.public_key(), &message, at).await;

        assert_eq!(event.kind, Kind::PrivateDirectMessage);
        assert_eq!(event.pubkey, node.public_key(), "authored by the node");
        assert_eq!(event.created_at, at, "created when the script says");
        assert!(event.verify().is_ok(), "a valid signature");
        let opened = unwrap_incoming(&event, &trade)
            .await
            .unwrap()
            .expect("opens with the addressed key");
        assert_eq!(opened.sender, node.public_key());
        assert_eq!(
            opened.message.get_inner_message_kind().action,
            Action::AddInvoice
        );
        assert_eq!(opened.message.get_inner_message_kind().id, Some(id(1)));
        let other = unwrap_incoming(&event, &stranger).await;
        assert!(
            !matches!(other, Ok(Some(_))),
            "a key it is not addressed to does not open it"
        );
    }

    #[tokio::test]
    async fn every_constructor_makes_a_message_that_round_trips() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let parked = order(1, Status::Pending);
        let cases: Vec<(Message, Action, Option<Uuid>)> = vec![
            (
                mail_pay_bond_invoice(&parked, "lnbcrt1bond"),
                Action::PayBondInvoice,
                Some(id(1)),
            ),
            (
                mail_add_invoice(&order(1, Status::WaitingBuyerInvoice)),
                Action::AddInvoice,
                Some(id(1)),
            ),
            (
                mail_pay_invoice(&order(1, Status::WaitingPayment), "lnbcrt1hold"),
                Action::PayInvoice,
                Some(id(1)),
            ),
            (
                mail_waiting_seller(id(2)),
                Action::WaitingSellerToPay,
                Some(id(2)),
            ),
            (
                mail_cant_do(CantDoReason::InvalidInvoice),
                Action::CantDo,
                None,
            ),
            (
                mail_bond_payout_request(&order(3, Status::Success), 1_792_000_000),
                Action::AddBondInvoice,
                Some(id(3)),
            ),
            (
                mail_text(id(4), "hello"),
                Action::HoldInvoicePaymentAccepted,
                Some(id(4)),
            ),
        ];
        for (message, action, order_id) in cases {
            let event = node_event(&node, trade.public_key(), &message, minutes_ago(1)).await;
            let opened = unwrap_incoming(&event, &trade)
                .await
                .unwrap()
                .expect("opens");
            let inner = opened.message.get_inner_message_kind();
            assert_eq!(
                (inner.action.clone(), inner.id),
                (action.clone(), order_id),
                "{action:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_payloads_arrive_intact() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let event = node_event(
            &node,
            trade.public_key(),
            &mail_pay_bond_invoice(&order(1, Status::Pending), "lnbcrt1bondinvoice"),
            minutes_ago(1),
        )
        .await;
        let opened = unwrap_incoming(&event, &trade).await.unwrap().unwrap();
        match &opened.message.get_inner_message_kind().payload {
            Some(Payload::PaymentRequest(Some(parked), invoice, None)) => {
                assert_eq!(
                    (parked.id, invoice.as_str()),
                    (Some(id(1)), "lnbcrt1bondinvoice")
                );
                assert_eq!(parked.amount, 6_233);
            }
            other => panic!("a payment request, got {other:?}"),
        }
    }

    async fn relay_with(events: &[Event]) -> (LoopbackRelay, Client) {
        let relay = LocalRelay::new();
        for event in events {
            relay.add_event(event.clone()).await.unwrap();
        }
        let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap();
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        (relay, client)
    }

    /// What `message list` will ask: one filter, the node as author, every key.
    fn mailbox_filter(node: PublicKey, keys: &[PublicKey], since: Timestamp) -> Filter {
        Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .author(node)
            .pubkeys(keys.iter().copied())
            .since(since)
            .limit(50)
    }

    #[tokio::test]
    async fn one_filter_with_several_keys_returns_the_messages_of_exactly_those_keys() {
        let (node, a, b, c) = (
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
        );
        let impostor = Keys::generate();
        let mail = mail_waiting_seller(id(1));
        let events = [
            node_event(&node, a.public_key(), &mail, minutes_ago(5)).await,
            node_event(&node, b.public_key(), &mail, minutes_ago(4)).await,
            node_event(&node, c.public_key(), &mail, minutes_ago(3)).await,
            node_event(&impostor, a.public_key(), &mail, minutes_ago(2)).await,
        ];
        let (_relay, client) = relay_with(&events).await;

        let filter = mailbox_filter(
            node.public_key(),
            &[a.public_key(), b.public_key()],
            minutes_ago(60),
        );
        let fetched = client
            .fetch_events(filter)
            .timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let ids: BTreeSet<_> = fetched.iter().map(|event| event.id).collect();
        let expected: BTreeSet<_> = [events[0].id, events[1].id].into_iter().collect();
        assert_eq!(ids, expected, "keys a and b from the node, nothing else");
    }

    #[tokio::test]
    async fn the_since_bound_of_the_filter_drops_older_messages() {
        let (node, a) = (Keys::generate(), Keys::generate());
        let mail = mail_waiting_seller(id(1));
        let old = node_event(&node, a.public_key(), &mail, minutes_ago(120)).await;
        let new = node_event(&node, a.public_key(), &mail, minutes_ago(10)).await;
        let (_relay, client) = relay_with(&[old, new.clone()]).await;

        let filter = mailbox_filter(node.public_key(), &[a.public_key()], minutes_ago(30));
        let fetched = client
            .fetch_events(filter)
            .timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let ids: Vec<_> = fetched.iter().map(|event| event.id).collect();
        assert_eq!(ids, [new.id]);
    }
}

// --- What `getdm` does today (removed with the command in slice 4) -----------

mod legacy_getdm {
    use super::*;
    use mostro_client::parser::dms::parse_dm_events;

    async fn events(node: &Keys, trade: &Keys, minutes: &[u64]) -> BTreeSet<Event> {
        let mut set = BTreeSet::new();
        for (n, minutes) in minutes.iter().enumerate() {
            let message = mail_waiting_seller(id(n as u128 + 1));
            set.insert(node_event(node, trade.public_key(), &message, minutes_ago(*minutes)).await);
        }
        set
    }

    fn order_ids(parsed: &[(Message, u64, PublicKey)]) -> Vec<Option<Uuid>> {
        parsed
            .iter()
            .map(|(message, ..)| message.get_inner_message_kind().id)
            .collect()
    }

    #[tokio::test]
    async fn messages_come_back_oldest_first_whatever_the_order_of_the_events() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        // Events 1, 2, 3 are created 30, 10 and 20 minutes ago.
        let set = events(&node, &trade, &[30, 10, 20]).await;

        let parsed = parse_dm_events(set, &trade, None, true).await;

        assert_eq!(order_ids(&parsed), [Some(id(1)), Some(id(3)), Some(id(2))]);
        assert!(parsed.windows(2).all(|pair| pair[0].1 <= pair[1].1));
    }

    #[tokio::test]
    async fn the_since_window_is_in_minutes_before_now() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let set = events(&node, &trade, &[200, 20, 5]).await;

        let wide = parse_dm_events(set.clone(), &trade, Some(&300), true).await;
        let narrow = parse_dm_events(set.clone(), &trade, Some(&30), true).await;
        let tiny = parse_dm_events(set.clone(), &trade, Some(&10), true).await;
        let negative = parse_dm_events(set, &trade, Some(&-5), true).await;

        assert_eq!(wide.len(), 3);
        assert_eq!(order_ids(&narrow), [Some(id(2)), Some(id(3))]);
        assert_eq!(order_ids(&tiny), [Some(id(3))]);
        assert!(
            negative.is_empty(),
            "a negative window is a cutoff in the future"
        );
    }

    #[tokio::test]
    async fn events_the_key_cannot_open_are_skipped_silently() {
        let (node, trade, other) = (Keys::generate(), Keys::generate(), Keys::generate());
        let mut set = events(&node, &trade, &[5]).await;
        set.extend(events(&node, &other, &[4]).await);

        let parsed = parse_dm_events(set, &trade, None, true).await;

        assert_eq!(parsed.len(), 1, "only the message addressed to the key");
    }

    #[tokio::test]
    async fn the_author_is_not_checked_here_only_by_the_relay_filter() {
        // `getdm` asks the relay for the node's events only; `parse_dm_events`
        // itself opens any event addressed to the key, whoever signed it. The
        // new command must check the author itself.
        let (node, impostor, trade) = (Keys::generate(), Keys::generate(), Keys::generate());
        let mut set = events(&node, &trade, &[5]).await;
        set.extend(events(&impostor, &trade, &[4]).await);

        let parsed = parse_dm_events(set, &trade, None, true).await;

        let senders: BTreeSet<_> = parsed.iter().map(|(_, _, sender)| *sender).collect();
        assert_eq!(
            senders,
            [node.public_key(), impostor.public_key()]
                .into_iter()
                .collect()
        );
    }

    #[tokio::test]
    async fn the_time_and_sender_of_each_message_are_reported() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let at = minutes_ago(7);
        let mut set = BTreeSet::new();
        set.insert(node_event(&node, trade.public_key(), &mail_waiting_seller(id(1)), at).await);

        let parsed = parse_dm_events(set, &trade, None, true).await;

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            (parsed[0].1, parsed[0].2),
            (at.as_secs(), node.public_key())
        );
    }

    #[tokio::test]
    async fn a_message_that_is_not_mostro_json_is_skipped() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let garbage =
            nostr_sdk::prelude::EventBuilder::new(Kind::PrivateDirectMessage, "not a message")
                .tags([nostr_sdk::prelude::Tag::public_key(trade.public_key())])
                .custom_created_at(minutes_ago(1))
                .finalize(&node)
                .unwrap();
        let mut set = BTreeSet::new();
        set.insert(garbage);

        let parsed = parse_dm_events(set, &trade, None, true).await;

        assert!(parsed.is_empty());
    }

    #[tokio::test]
    async fn only_kind_14_is_read() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let wrong_kind = nostr_sdk::prelude::EventBuilder::new(Kind::TextNote, "x")
            .tags([nostr_sdk::prelude::Tag::public_key(trade.public_key())])
            .finalize(&node)
            .unwrap();
        let mut set = BTreeSet::new();
        set.insert(wrong_kind);

        assert!(parse_dm_events(set, &trade, None, true).await.is_empty());
    }
}
