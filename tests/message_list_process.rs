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
#[path = "common/schema_check.rs"]
mod schema_check;

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

// --- The trade keys of the store (spec 006, slice 2) -------------------------

mod store {
    use super::fake_mostro::{seed_identity, set_last_trade_index};
    use super::harness::Sandbox;
    use mostro_client::store::{Store, StoreError};

    async fn stored(last: i64) -> (Sandbox, Store) {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        set_last_trade_index(&sandbox.mcli_dir(), last).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        (sandbox, store)
    }

    fn database(sandbox: &Sandbox) -> Vec<u8> {
        std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap()
    }

    #[tokio::test]
    async fn the_keys_of_every_index_up_to_the_last_come_newest_first() {
        let (_sandbox, store) = stored(4).await;

        let derived = store.trade_keys(200).await.unwrap();

        let indexes: Vec<i64> = derived.keys.iter().map(|(index, _)| *index).collect();
        assert_eq!(indexes, [4, 3, 2, 1]);
        assert_eq!(derived.not_read, 0);
        let distinct: std::collections::BTreeSet<_> = derived
            .keys
            .iter()
            .map(|(_, keys)| keys.public_key())
            .collect();
        assert_eq!(distinct.len(), 4, "one key per index");
    }

    #[tokio::test]
    async fn the_derivation_is_the_one_the_commands_use_for_the_next_trade() {
        let (sandbox, store) = stored(3).await;
        let next = store.next_trade().await.unwrap();
        assert_eq!(next.index, 4);

        set_last_trade_index(&sandbox.mcli_dir(), 4).await;
        let derived = store.trade_keys(200).await.unwrap();

        let (index, keys) = &derived.keys[0];
        assert_eq!(*index, 4);
        assert_eq!(keys.public_key(), next.keys.public_key());
    }

    #[tokio::test]
    async fn only_the_newest_are_derived_and_the_rest_are_counted() {
        let (_sandbox, store) = stored(10).await;

        let few = store.trade_keys(3).await.unwrap();
        let all = store.trade_keys(10).await.unwrap();
        let more = store.trade_keys(50).await.unwrap();
        let none = store.trade_keys(0).await.unwrap();

        let indexes = |t: &mostro_client::store::TradeKeys| -> Vec<i64> {
            t.keys.iter().map(|(index, _)| *index).collect()
        };
        assert_eq!((indexes(&few), few.not_read), (vec![10, 9, 8], 7));
        assert_eq!((all.keys.len(), all.not_read), (10, 0));
        assert_eq!((more.keys.len(), more.not_read), (10, 0));
        assert_eq!((none.keys.len(), none.not_read), (0, 10));
    }

    #[tokio::test]
    async fn no_trade_yet_means_no_keys() {
        let (_sandbox, store) = stored(0).await;

        let derived = store.trade_keys(200).await.unwrap();

        assert!(derived.keys.is_empty());
        assert_eq!(derived.not_read, 0);
    }

    #[tokio::test]
    async fn reading_the_keys_writes_nothing_and_never_shows_them() {
        let (sandbox, store) = stored(3).await;
        let before = database(&sandbox);

        let derived = store.trade_keys(200).await.unwrap();

        assert_eq!(database(&sandbox), before, "no byte changed");
        let shown = format!("{derived:?}");
        assert!(
            shown.contains("indexes") && shown.contains("[3, 2, 1]"),
            "{shown}"
        );
        for (_, keys) in &derived.keys {
            assert!(!shown.contains(&keys.secret_key().to_secret_hex()));
            assert!(!shown.contains(&keys.public_key().to_hex()));
        }
    }

    #[tokio::test]
    async fn a_missing_store_or_user_is_missing_and_a_damaged_one_is_unreadable() {
        let sandbox = Sandbox::new().unwrap();
        let store = Store::in_dir(&sandbox.mcli_dir());
        assert_eq!(
            store.trade_keys(200).await.unwrap_err(),
            StoreError::Missing
        );
        assert!(
            !sandbox.mcli_dir().join("mcli.db").exists(),
            "never created"
        );

        let (sandbox, store) = stored(2).await;
        let options =
            sqlx::sqlite::SqliteConnectOptions::new().filename(sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DELETE FROM users")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert_eq!(
            store.trade_keys(200).await.unwrap_err(),
            StoreError::Missing
        );

        let (sandbox, store) = stored(2).await;
        std::fs::write(sandbox.mcli_dir().join("mcli.db"), b"not a database").unwrap();
        let error = store.trade_keys(200).await.unwrap_err();
        assert!(
            matches!(error, StoreError::Unreadable | StoreError::Unwritable),
            "{error:?}"
        );

        let (sandbox, store) = stored(2).await;
        let options =
            sqlx::sqlite::SqliteConnectOptions::new().filename(sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("UPDATE users SET mnemonic = 'not a mnemonic'")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert_eq!(
            store.trade_keys(200).await.unwrap_err(),
            StoreError::Unreadable
        );
    }
}

// --- The operation (spec 006, slice 3) ---------------------------------------

mod operation {
    use super::fake_mostro::{
        mail_add_invoice, mail_bond_payout_request, mail_cant_do, mail_pay_bond_invoice,
        mail_pay_invoice, mail_text, mail_waiting_seller, node_event, seed_identity,
        set_last_trade_index,
    };
    use super::harness::Sandbox;
    use super::*;
    use chrono::Utc;
    use mostro_client::commands::message::list::{message_list, ListOutcome};
    use mostro_client::msg::{PublicReadConfig, ReadLimits};
    use mostro_client::outcome::messages::{MessageListInput, NextStep};
    use mostro_client::outcome::{ErrorCode, Status as Outcome};
    use mostro_client::store::{Store, Trade};
    use serde_json::{json, Value};

    struct Mailroom {
        sandbox: Sandbox,
        store: Store,
        node: Keys,
        relays: Vec<LoopbackRelay>,
        /// The keys of the user, newest first, as the command derives them.
        keys: Vec<(i64, Keys)>,
    }

    impl Mailroom {
        /// A user with `last` trades and `relays` relays, each holding `events`
        /// (a function of the node, the keys and the relay number).
        async fn with<F, Fut>(last: i64, relays: usize, events: F) -> Self
        where
            F: Fn(Keys, Vec<(i64, Keys)>, usize) -> Fut,
            Fut: std::future::Future<Output = Vec<Event>>,
        {
            let sandbox = Sandbox::new().unwrap();
            seed_identity(&sandbox.mcli_dir()).await;
            set_last_trade_index(&sandbox.mcli_dir(), last).await;
            let store = Store::in_dir(&sandbox.mcli_dir());
            let keys = store.trade_keys(1000).await.unwrap().keys;
            let node = Keys::generate();
            let mut running = Vec::new();
            for number in 0..relays {
                let relay = LocalRelay::new();
                for event in events(node.clone(), keys.clone(), number).await {
                    relay.add_event(event).await.unwrap();
                }
                running.push(
                    LoopbackRelay::start(Backend::Relay(Box::new(relay)))
                        .await
                        .unwrap(),
                );
            }
            Self {
                sandbox,
                store,
                node,
                relays: running,
                keys,
            }
        }

        fn key(&self, index: i64) -> &Keys {
            &self.keys.iter().find(|(i, _)| *i == index).unwrap().1
        }

        fn config(&self) -> PublicReadConfig {
            let urls: Vec<&str> = self.relays.iter().map(|r| r.url.as_str()).collect();
            PublicReadConfig::new(self.node.public_key(), &urls.join(",")).unwrap()
        }

        fn database(&self) -> Vec<u8> {
            std::fs::read(self.sandbox.mcli_dir().join("mcli.db")).unwrap()
        }

        async fn list(&self, since: Option<i64>, order: Option<Uuid>) -> ListOutcome {
            let input = MessageListInput::validate(since, order).unwrap();
            message_list(&input, &self.config(), &self.store, &limits(), Utc::now()).await
        }
    }

    fn limits() -> ReadLimits {
        ReadLimits {
            connect: Duration::from_secs(2),
            read: Duration::from_millis(1_500),
            cleanup: Duration::from_millis(500),
            ..ReadLimits::default()
        }
    }

    fn document(outcome: &ListOutcome) -> Value {
        serde_json::to_value(outcome).unwrap()
    }

    fn actions(outcome: &ListOutcome) -> Vec<String> {
        outcome
            .data()
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.action().to_string())
            .collect()
    }

    /// One message of every kind the flow produces, to the keys of indexes 3, 2
    /// and 1, in a known order of time.
    async fn flow(node: Keys, keys: Vec<(i64, Keys)>, _: usize) -> Vec<Event> {
        let to = |index: i64| {
            keys.iter()
                .find(|(i, _)| *i == index)
                .map(|(_, key)| key.public_key())
                .unwrap()
        };
        let parked = order(1, Status::Pending);
        vec![
            node_event(
                &node,
                to(3),
                &mail_pay_bond_invoice(&parked, "lnbcrt1bond"),
                minutes_ago(25),
            )
            .await,
            node_event(
                &node,
                to(3),
                &mail_add_invoice(&order(1, Status::WaitingBuyerInvoice)),
                minutes_ago(20),
            )
            .await,
            node_event(&node, to(2), &mail_waiting_seller(id(2)), minutes_ago(15)).await,
            node_event(
                &node,
                to(2),
                &mail_pay_invoice(&order(2, Status::WaitingPayment), "lnbcrt1hold"),
                minutes_ago(10),
            )
            .await,
            node_event(
                &node,
                to(1),
                &mail_cant_do(CantDoReason::InvalidPeer),
                minutes_ago(8),
            )
            .await,
            node_event(
                &node,
                to(1),
                &mail_bond_payout_request(&order(3, Status::Success), 1_792_000_000),
                minutes_ago(6),
            )
            .await,
            node_event(&node, to(1), &mail_text(id(4), "hello"), minutes_ago(4)).await,
        ]
    }

    #[tokio::test]
    async fn every_message_of_the_flow_comes_back_oldest_first_with_its_next_step() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.database();

        let outcome = room.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(
            actions(&outcome),
            [
                "pay-bond-invoice",
                "add-invoice",
                "waiting-seller-to-pay",
                "pay-invoice",
                "cant-do",
                "add-bond-invoice",
                "hold-invoice-payment-accepted",
            ]
        );
        let data = outcome.data().unwrap();
        let steps: Vec<_> = data.messages().iter().map(|m| m.next_step()).collect();
        assert_eq!(
            steps,
            [
                Some(NextStep::PayBondInvoice),
                Some(NextStep::AddInvoice),
                None,
                Some(NextStep::PayHoldInvoice),
                None,
                Some(NextStep::AddBondInvoice),
                None,
            ]
        );
        let indexes: Vec<_> = data
            .messages()
            .iter()
            .map(|m| m.trade_index().map(str::to_string))
            .collect();
        assert_eq!(
            indexes,
            ["3", "3", "2", "2", "1", "1", "1"].map(|i| Some(i.to_string()))
        );
        assert_eq!(
            data.messages()[0].trade_pubkey(),
            room.key(3).public_key().to_hex()
        );
        assert!(!data.truncated());
        assert_eq!((data.keys_read(), data.keys_not_read()), ("3", "0"));
        assert_eq!(data.since_minutes(), "30");
        let doc = document(&outcome);
        assert_eq!(doc["meta"]["publication"], "not_applicable");
        assert_eq!(doc["meta"]["persistence"], "not_applicable");
        assert_eq!(doc["meta"]["snapshot"]["configured_relays"], 1);
        assert_eq!(doc["meta"]["snapshot"]["completed_relays"], 1);
        assert_eq!(doc["meta"]["snapshot"]["received_events"], 7);
        assert_eq!(doc["warnings"], json!([]));
        assert_eq!(room.database(), before, "a read writes nothing");
    }

    #[tokio::test]
    async fn the_bond_invoice_and_the_hold_invoice_are_data_of_their_records() {
        let room = Mailroom::with(3, 1, flow).await;

        let doc = document(&room.list(None, None).await);

        let messages = doc["data"]["messages"].as_array().unwrap();
        assert_eq!(messages[0]["payload"]["kind"], "payment_request");
        assert_eq!(messages[0]["payload"]["invoice"], "lnbcrt1bond");
        assert_eq!(messages[0]["payload"]["order"]["sats_amount"], "6233");
        assert_eq!(messages[3]["payload"]["invoice"], "lnbcrt1hold");
        assert_eq!(
            messages[4]["payload"],
            json!({"kind": "cant_do", "reason": "invalid_peer"})
        );
        assert_eq!(messages[5]["payload"]["kind"], "bond_payout_request");
        assert_eq!(
            messages[6]["payload"],
            json!({"kind": "text", "text": "hello"})
        );
    }

    #[tokio::test]
    async fn the_same_mailbox_read_twice_gives_the_same_answer() {
        let room = Mailroom::with(3, 1, flow).await;

        let first = room.list(None, None).await;
        let second = room.list(None, None).await;

        let strip = |outcome: &ListOutcome| {
            let mut doc = document(outcome);
            // The window moves with the clock; the messages do not.
            doc["meta"]["snapshot"]["observed_at"] = Value::Null;
            doc["meta"]["snapshot"]["since"] = Value::Null;
            doc
        };
        assert_eq!(strip(&first), strip(&second));
    }

    #[tokio::test]
    async fn what_is_not_the_nodes_mail_for_these_keys_in_this_window_never_appears() {
        let room = Mailroom::with(2, 1, |node, keys, _| async move {
            let (mine, other) = (keys[0].1.public_key(), Keys::generate().public_key());
            let impostor = Keys::generate();
            let mail = mail_waiting_seller(id(1));
            let garbage =
                nostr_sdk::prelude::EventBuilder::new(Kind::PrivateDirectMessage, "not a message")
                    .tags([nostr_sdk::prelude::Tag::public_key(mine)])
                    .custom_created_at(minutes_ago(3))
                    .finalize(&node)
                    .unwrap();
            vec![
                node_event(&node, mine, &mail_waiting_seller(id(9)), minutes_ago(5)).await,
                node_event(&node, other, &mail, minutes_ago(5)).await,
                node_event(&impostor, mine, &mail, minutes_ago(5)).await,
                node_event(&node, mine, &mail, minutes_ago(300)).await,
                garbage,
            ]
        })
        .await;

        let outcome = room.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.messages().len(), 1, "{:?}", actions(&outcome));
        assert_eq!(
            data.messages()[0].order_id(),
            Some(id(9).to_string().as_str())
        );
    }

    #[tokio::test]
    async fn the_window_decides_which_messages_are_read() {
        let room = Mailroom::with(1, 1, |node, keys, _| async move {
            let to = keys[0].1.public_key();
            vec![
                node_event(&node, to, &mail_waiting_seller(id(1)), minutes_ago(200)).await,
                node_event(&node, to, &mail_waiting_seller(id(2)), minutes_ago(20)).await,
                node_event(&node, to, &mail_waiting_seller(id(3)), minutes_ago(5)).await,
            ]
        })
        .await;

        let counts = [
            room.list(Some(10), None)
                .await
                .data()
                .unwrap()
                .messages()
                .len(),
            room.list(Some(30), None)
                .await
                .data()
                .unwrap()
                .messages()
                .len(),
            room.list(None, None).await.data().unwrap().messages().len(),
            room.list(Some(300), None)
                .await
                .data()
                .unwrap()
                .messages()
                .len(),
        ];

        assert_eq!(counts, [1, 2, 2, 3]);
    }

    #[tokio::test]
    async fn the_same_event_from_two_relays_appears_once() {
        let room = Mailroom::with(3, 2, |node, keys, _| async move {
            // Both relays hold the same events: identical ids.
            flow(node, keys, 0).await
        })
        .await;
        // The generator ran twice with different events; make both relays equal.
        let shared = flow(room.node.clone(), room.keys.clone(), 0).await;
        let both = Mailroom {
            relays: {
                let mut relays = Vec::new();
                for _ in 0..2 {
                    let relay = LocalRelay::new();
                    for event in &shared {
                        relay.add_event(event.clone()).await.unwrap();
                    }
                    relays.push(
                        LoopbackRelay::start(Backend::Relay(Box::new(relay)))
                            .await
                            .unwrap(),
                    );
                }
                relays
            },
            ..room
        };

        let outcome = both.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(outcome.data().unwrap().messages().len(), 7, "once each");
        let doc = document(&outcome);
        assert_eq!(doc["meta"]["snapshot"]["configured_relays"], 2);
        assert_eq!(doc["meta"]["snapshot"]["completed_relays"], 2);
    }

    #[tokio::test]
    async fn an_order_reads_only_its_own_key_and_knows_its_index() {
        let room = Mailroom::with(3, 1, flow).await;
        let trade = Trade {
            index: 2,
            keys: room.key(2).clone(),
        };
        room.store
            .save_order(&order(2, Status::WaitingPayment), &trade, 1)
            .await
            .unwrap();

        let outcome = room.list(None, Some(id(2))).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(actions(&outcome), ["waiting-seller-to-pay", "pay-invoice"]);
        let data = outcome.data().unwrap();
        assert!(data.messages().iter().all(|m| m.trade_index() == Some("2")));
        assert_eq!((data.keys_read(), data.keys_not_read()), ("1", "0"));
    }

    #[tokio::test]
    async fn an_unknown_order_asks_nothing() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.database();

        let outcome = room.list(None, Some(id(77))).await;

        assert_eq!(outcome.status(), Outcome::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::OrderUnknown));
        assert_eq!(room.relays[0].connections(), 0, "no relay was contacted");
        assert_eq!(room.database(), before);
    }

    #[tokio::test]
    async fn only_the_newest_two_hundred_keys_are_asked_about_and_the_rest_are_counted() {
        let room = Mailroom::with(250, 1, |node, keys, _| async move {
            let key = |index: i64| {
                keys.iter()
                    .find(|(i, _)| *i == index)
                    .unwrap()
                    .1
                    .public_key()
            };
            vec![
                node_event(
                    &node,
                    key(250),
                    &mail_waiting_seller(id(250)),
                    minutes_ago(5),
                )
                .await,
                node_event(&node, key(51), &mail_waiting_seller(id(51)), minutes_ago(4)).await,
                node_event(&node, key(50), &mail_waiting_seller(id(50)), minutes_ago(3)).await,
            ]
        })
        .await;

        let outcome = room.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        let orders: Vec<_> = data
            .messages()
            .iter()
            .filter_map(|m| m.order_id())
            .collect();
        assert_eq!(
            orders,
            [id(250).to_string(), id(51).to_string()],
            "key 50 is out of reach"
        );
        assert_eq!((data.keys_read(), data.keys_not_read()), ("200", "50"));
        let doc = document(&outcome);
        assert_eq!(doc["warnings"][0]["code"], "trade_keys_limited");
    }

    #[tokio::test]
    async fn an_order_older_than_the_key_cap_is_still_read_with_an_unknown_index() {
        let room = Mailroom::with(250, 1, |node, keys, _| async move {
            let old = keys.iter().find(|(i, _)| *i == 10).unwrap().1.public_key();
            vec![node_event(&node, old, &mail_waiting_seller(id(10)), minutes_ago(2)).await]
        })
        .await;
        let trade = Trade {
            index: 10,
            keys: room.key(10).clone(),
        };
        room.store
            .save_order(&order(10, Status::WaitingPayment), &trade, 1)
            .await
            .unwrap();

        let outcome = room.list(None, Some(id(10))).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.messages().len(), 1);
        assert_eq!(data.messages()[0].trade_index(), None);
        assert_eq!(data.keys_not_read(), "0");
    }

    #[tokio::test]
    async fn a_busy_mailbox_is_cut_at_the_relay_limit_and_says_so() {
        let room = Mailroom::with(1, 1, |node, keys, _| async move {
            let to = keys[0].1.public_key();
            let mut events = Vec::new();
            for n in 0..60u64 {
                events.push(
                    node_event(
                        &node,
                        to,
                        &mail_waiting_seller(id(n as u128 + 1)),
                        Timestamp::from(Timestamp::now().as_secs() - 3_000 + n * 10),
                    )
                    .await,
                );
            }
            events
        })
        .await;

        let outcome = room.list(Some(120), None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.messages().len(), 50, "the relay's limit");
        assert!(data.truncated());
        let times: Vec<_> = data
            .messages()
            .iter()
            .map(|m| m.created_at_unix().parse::<u64>().unwrap())
            .collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "oldest first");
        assert_eq!(
            data.messages()[49].order_id(),
            Some(id(60).to_string().as_str()),
            "the newest are kept"
        );
    }

    #[tokio::test]
    async fn the_record_cap_keeps_the_newest_two_hundred() {
        let room = Mailroom::with(1, 5, |node, keys, relay| async move {
            let to = keys[0].1.public_key();
            let mut events = Vec::new();
            for n in 0..50u64 {
                let at = Timestamp::from(
                    Timestamp::now().as_secs() - 3_000 + (relay as u64 * 50 + n) * 5,
                );
                events.push(
                    node_event(
                        &node,
                        to,
                        &mail_waiting_seller(id((relay as u64 * 50 + n) as u128 + 1)),
                        at,
                    )
                    .await,
                );
            }
            events
        })
        .await;

        let outcome = room.list(Some(120), None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.messages().len(), 200);
        assert!(data.truncated());
        assert_eq!(
            data.messages()[199].order_id(),
            Some(id(250).to_string().as_str())
        );
        assert_eq!(
            data.messages()[0].order_id(),
            Some(id(51).to_string().as_str()),
            "the 50 oldest are dropped"
        );
    }

    #[tokio::test]
    async fn a_user_with_no_trade_yet_gets_an_empty_answer_and_no_relay_is_asked() {
        let room = Mailroom::with(0, 1, flow_none).await;

        let outcome = room.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert!(data.messages().is_empty() && !data.truncated());
        assert_eq!((data.keys_read(), data.keys_not_read()), ("0", "0"));
        assert_eq!(
            room.relays[0].connections(),
            0,
            "a filter with no recipient would read everyone's mail"
        );
        assert_eq!(
            document(&outcome)["meta"]["snapshot"]["completed_relays"],
            0
        );
    }

    async fn flow_none(_: Keys, _: Vec<(i64, Keys)>, _: usize) -> Vec<Event> {
        Vec::new()
    }

    #[tokio::test]
    async fn an_empty_window_is_a_success() {
        let room = Mailroom::with(2, 1, flow_none).await;

        let outcome = room.list(None, None).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert!(outcome.data().unwrap().messages().is_empty());
        assert_eq!(
            document(&outcome)["meta"]["snapshot"]["completed_relays"],
            1
        );
    }

    #[tokio::test]
    async fn failures_name_their_code_carry_the_evidence_they_have_and_write_nothing() {
        // A relay that never answers: the read deadline, with a snapshot.
        let room = Mailroom::with(2, 1, flow_none).await;
        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let config = PublicReadConfig::new(room.node.public_key(), &silent.url).unwrap();
        let before = room.database();
        let input = MessageListInput::validate(None, None).unwrap();
        let outcome = message_list(&input, &config, &room.store, &limits(), Utc::now()).await;
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReadTimeout));
        assert!(document(&outcome)["meta"]["snapshot"].is_object());

        // A relay nobody listens on: before the query, no snapshot.
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);
        let config = PublicReadConfig::new(room.node.public_key(), &dead).unwrap();
        let outcome = message_list(&input, &config, &room.store, &limits(), Utc::now()).await;
        assert_eq!(outcome.error_code(), Some(ErrorCode::RelayConnectionFailed));
        assert_eq!(document(&outcome)["meta"]["snapshot"], Value::Null);
        assert_eq!(room.database(), before);

        // Store problems.
        let nowhere = Store::in_dir(&room.sandbox.mcli_dir().join("absent"));
        let outcome = message_list(&input, &room.config(), &nowhere, &limits(), Utc::now()).await;
        assert_eq!(outcome.error_code(), Some(ErrorCode::IdentityMissing));
        std::fs::write(room.sandbox.mcli_dir().join("mcli.db"), b"not a database").unwrap();
        let outcome =
            message_list(&input, &room.config(), &room.store, &limits(), Utc::now()).await;
        assert_eq!(outcome.error_code(), Some(ErrorCode::StoreUnreadable));
    }

    // --- The binary (slice 4b) ---

    use super::harness::{closed_pipe, run_with_watchdog, Captured, WATCHDOG};

    const BOND_INVOICE: &str = "lnbcrt1bond";

    impl Mailroom {
        fn command(&self, args: &[&str]) -> tokio::process::Command {
            let mut command = self.sandbox.command(args);
            let relays: Vec<&LoopbackRelay> = self.relays.iter().collect();
            super::harness::set_node_env(&mut command, &self.node, &relays);
            command
        }

        async fn run(&self, args: &[&str]) -> Captured {
            let run = run_with_watchdog(self.command(args), WATCHDOG)
                .await
                .unwrap();
            assert!(run.reaped && run.status.is_some(), "{args:?}");
            run
        }
    }

    fn one_document(run: &Captured) -> Value {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        serde_json::from_str(&stdout).unwrap()
    }

    fn conforms_to_the_checked_in_schema(outcome: &Value) {
        let path = format!(
            "{}/docs/schemas/message-list-v1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            super::schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    #[tokio::test]
    async fn json_returns_the_whole_flow_as_one_document_and_changes_nothing() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.database();

        let run = room
            .run(&["--output", "json", "--verbose", "message", "list"])
            .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(run.stderr_text(), "verbose: message.list ok\n");
        let outcome = one_document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["command"], "message.list");
        assert_eq!(outcome["status"], "ok");
        let data = &outcome["data"];
        assert_eq!(data["messages"].as_array().unwrap().len(), 7);
        assert_eq!(data["truncated"], false);
        assert_eq!(data["since_minutes"], "30");
        assert_eq!(data["node_pubkey"], room.node.public_key().to_hex());
        let first = &data["messages"][0];
        assert_eq!(first["next_step"], "pay_bond_invoice");
        assert_eq!(first["payload"]["invoice"], BOND_INVOICE);
        assert_eq!(
            outcome["meta"]["publication"], "not_applicable",
            "{outcome}"
        );
        assert_eq!(outcome["meta"]["snapshot"]["completed_relays"], 1);
        assert_eq!(room.database(), before, "a read writes nothing");
        let secrets: Vec<String> = room
            .keys
            .iter()
            .map(|(_, key)| key.secret_key().to_secret_hex())
            .collect();
        for stream in [run.stdout_text(), run.stderr_text()] {
            for secret in &secrets {
                assert!(!stream.contains(secret), "a trade key leaked");
            }
            assert!(!stream.contains("127.0.0.1"), "a relay address leaked");
        }
    }

    #[tokio::test]
    async fn json_needs_no_yes_and_never_reads_stdin() {
        let room = Mailroom::with(3, 1, flow).await;
        let mut command = room.command(&["--output", "json", "message", "list"]);
        // A stdin that fails when read: the write end of a closed pipe.
        command.stdin(closed_pipe());

        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(one_document(&run)["status"], "ok");
    }

    #[tokio::test]
    async fn human_shows_each_invoice_alone_on_its_line_and_says_what_to_do_next() {
        let room = Mailroom::with(3, 1, flow).await;

        let run = room.run(&["message", "list"]).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let text = run.stdout_text();
        for invoice in [BOND_INVOICE, "lnbcrt1hold"] {
            assert!(text.lines().any(|line| line == invoice), "{text}");
        }
        assert!(text.contains("Next: pay the bond invoice"), "{text}");
        assert!(text.contains("`order add-invoice --order <id>"), "{text}");
        assert!(text.contains("`bond add-invoice --order <id>"), "{text}");
        assert!(text.contains("Refused: invalid_peer"), "{text}");
        let bond = text.find("pay-bond-invoice").unwrap();
        let hold = text.find("Action: pay-invoice").unwrap();
        assert!(bond < hold, "oldest first");
    }

    #[tokio::test]
    async fn the_window_and_the_order_narrow_the_result() {
        let room = Mailroom::with(3, 1, flow).await;
        let trade = Trade {
            index: 2,
            keys: room.key(2).clone(),
        };
        room.store
            .save_order(&order(2, Status::WaitingPayment), &trade, 1)
            .await
            .unwrap();
        let before = room.database();

        let narrow = room
            .run(&["--output", "json", "message", "list", "--since", "9"])
            .await;
        let for_order = room
            .run(&[
                "--output",
                "json",
                "message",
                "list",
                "--order",
                &id(2).to_string(),
            ])
            .await;

        let narrow = one_document(&narrow);
        assert_eq!(narrow["data"]["since_minutes"], "9");
        assert_eq!(narrow["data"]["messages"].as_array().unwrap().len(), 3);
        let for_order = one_document(&for_order);
        let actions: Vec<&str> = for_order["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["action"].as_str().unwrap())
            .collect();
        assert_eq!(actions, ["waiting-seller-to-pay", "pay-invoice"]);
        assert_eq!(room.database(), before);
    }

    #[tokio::test]
    async fn bad_arguments_exit_2_before_any_relay_is_contacted() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.database();
        for since in ["0", "10081", "-3"] {
            let run = room
                .run(&[
                    "--output",
                    "json",
                    "message",
                    "list",
                    &format!("--since={since}"),
                ])
                .await;
            assert_eq!(run.code(), Some(2), "{since}");
            let outcome = one_document(&run);
            assert_eq!(outcome["error"]["code"], "invalid_arguments", "{since}");
            assert_eq!(outcome["command"], "message.list");
            assert_eq!(outcome["meta"]["publication"], "not_applicable");
        }
        let human = room.run(&["message", "list", "--since", "0"]).await;
        assert_eq!(human.code(), Some(2));
        assert!(human.stdout.is_empty());
        assert!(human.stderr_text().starts_with("error: "));
        let unparsable = room
            .run(&["--output", "json", "message", "list", "--since", "soon"])
            .await;
        assert_eq!(unparsable.code(), Some(2));
        assert_eq!(
            one_document(&unparsable)["error"]["code"],
            "invalid_arguments"
        );
        assert_eq!(room.relays[0].connections(), 0, "no relay was contacted");
        assert_eq!(room.database(), before);
    }

    #[tokio::test]
    async fn an_unknown_order_and_a_missing_identity_are_distinct_errors() {
        let room = Mailroom::with(3, 1, flow).await;
        let run = room
            .run(&[
                "--output",
                "json",
                "message",
                "list",
                "--order",
                &id(99).to_string(),
            ])
            .await;
        assert_eq!(run.code(), Some(1));
        assert_eq!(one_document(&run)["error"]["code"], "order_unknown");
        assert_eq!(room.relays[0].connections(), 0);

        let bare = Sandbox::new().unwrap();
        let mut command = bare.command(["--output", "json", "message", "list"]);
        super::harness::set_node_env(&mut command, &room.node, &[&room.relays[0]]);
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        assert_eq!(one_document(&run)["error"]["code"], "identity_missing");
        assert!(
            super::harness::is_empty_dir(&bare.mcli_dir()),
            "a read never creates the database"
        );
    }

    #[tokio::test]
    async fn the_old_command_is_gone_with_no_alias() {
        let room = Mailroom::with(3, 1, flow).await;

        let json = room
            .run(&["--output", "json", "getdm", "--since", "5"])
            .await;
        assert_eq!(json.code(), Some(2));
        let outcome = one_document(&json);
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
        assert_eq!(outcome["command"], Value::Null);

        let human = room.run(&["getdm"]).await;
        assert_eq!(human.code(), Some(2));
        assert!(human.stdout.is_empty());
        assert!(human.stderr_text().contains("unrecognized subcommand"));
        assert_eq!(room.relays[0].connections(), 0);
    }

    /// The command writes nothing, so a store that refuses writes is enough.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_read_only_store_is_read_without_complaint() {
        use std::os::unix::fs::PermissionsExt;
        let room = Mailroom::with(3, 1, flow).await;
        let database = room.sandbox.mcli_dir().join("mcli.db");
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(
            room.sandbox.mcli_dir(),
            std::fs::Permissions::from_mode(0o555),
        )
        .unwrap();

        let run = room.run(&["--output", "json", "message", "list"]).await;

        std::fs::set_permissions(
            room.sandbox.mcli_dir(),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(one_document(&run)["status"], "ok");
    }

    // --- The failure matrix (slice 5) ---

    const SENTINEL: &str = "SENTINEL-zq81";

    /// How the store of the sandbox is damaged before the run.
    #[derive(Clone, Copy, Debug)]
    enum Tweak {
        None,
        Absent,
        Garbage,
        EmptyUsers,
        NoUsersTable,
        NullMnemonic,
    }

    async fn tweak(room: &Mailroom, how: Tweak) {
        let path = room.sandbox.mcli_dir().join("mcli.db");
        let sql = |statement: &'static str| {
            let path = path.clone();
            async move {
                let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
                let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
                sqlx::query(statement).execute(&pool).await.unwrap();
                pool.close().await;
            }
        };
        match how {
            Tweak::None => {}
            Tweak::Absent => std::fs::remove_file(&path).unwrap(),
            Tweak::Garbage => std::fs::write(&path, b"this is not a sqlite database").unwrap(),
            Tweak::EmptyUsers => sql("DELETE FROM users").await,
            Tweak::NoUsersTable => sql("DROP TABLE users").await,
            Tweak::NullMnemonic => sql("UPDATE users SET mnemonic = NULL").await,
        }
    }

    struct Failing {
        label: &'static str,
        tweak: Tweak,
        set: &'static [(&'static str, &'static str)],
        unset: &'static [&'static str],
        code: &'static str,
    }

    impl Failing {
        const fn new(label: &'static str, tweak: Tweak, code: &'static str) -> Self {
            Self {
                label,
                tweak,
                set: &[],
                unset: &[],
                code,
            }
        }
    }

    impl Mailroom {
        async fn run_env(&self, args: &[&str], set: &[(&str, &str)], unset: &[&str]) -> Captured {
            let mut command = self.command(args);
            for (name, value) in set {
                command.env(name, value);
            }
            for name in unset {
                command.env_remove(name);
            }
            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
            assert!(run.reaped && run.status.is_some(), "{args:?}");
            run
        }

        fn files(&self) -> Vec<(String, Vec<u8>)> {
            let mut files: Vec<_> = std::fs::read_dir(self.sandbox.mcli_dir())
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        std::fs::read(entry.path()).unwrap_or_default(),
                    )
                })
                .collect();
            files.sort();
            files
        }
    }

    const READ_STAGES: &str = r#"{"publication": "not_applicable", "daemon_acceptance": "not_applicable",
                                  "persistence": "not_applicable", "snapshot": null}"#;

    /// Run `case` as JSON, human and verbose; every mode shows the same code,
    /// nothing leaks, no relay is contacted and the store is as it was.
    async fn check_failure(case: &Failing) {
        let room = Mailroom::with(3, 1, flow).await;
        tweak(&room, case.tweak).await;
        let before = room.files();
        let label = case.label;
        let stages: Value = serde_json::from_str(READ_STAGES).unwrap();
        for mode in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
            let mut args: Vec<&str> = mode.to_vec();
            args.extend(["message", "list"]);
            let run = room.run_env(&args, case.set, case.unset).await;
            assert_eq!(
                run.code(),
                Some(1),
                "{label} {mode:?}: {}",
                run.stderr_text()
            );
            let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
            for stream in [&stdout, &stderr] {
                assert!(!stream.contains("SENTINEL"), "{label}: a sentinel leaked");
                assert!(
                    !stream.contains("127.0.0.1"),
                    "{label}: a relay address leaked"
                );
                assert!(!stream.contains(BOND_INVOICE), "{label}: an invoice leaked");
            }
            if mode == ["--output", "json"] {
                let outcome = one_document(&run);
                conforms_to_the_checked_in_schema(&outcome);
                assert_eq!(outcome["command"], "message.list", "{label}");
                assert_eq!(outcome["status"], "error", "{label}");
                assert_eq!(outcome["data"], Value::Null, "{label}");
                assert_eq!(outcome["error"]["code"], case.code, "{label}");
                assert_eq!(outcome["meta"], stages, "{label}");
                assert!(stderr.is_empty(), "{label}: {stderr}");
            } else {
                assert!(stdout.is_empty(), "{label} {mode:?}: {stdout}");
                assert!(stderr.starts_with("error: "), "{label}: {stderr}");
            }
        }
        assert_eq!(
            room.relays[0].connections(),
            0,
            "{label}: a relay was contacted"
        );
        assert_eq!(room.files(), before, "{label}: the store changed");
        assert!(
            super::harness::is_empty_dir(&room.sandbox.home())
                && super::harness::is_empty_dir(&room.sandbox.cwd()),
            "{label}: files were created elsewhere"
        );
    }

    #[tokio::test]
    async fn failures_before_any_relay_are_stable_and_leave_no_trace() {
        let cases = [
            Failing::new("no store", Tweak::Absent, "identity_missing"),
            Failing::new("no user", Tweak::EmptyUsers, "identity_missing"),
            Failing::new("not a database", Tweak::Garbage, "identity_unreadable"),
            Failing::new("no users table", Tweak::NoUsersTable, "identity_unreadable"),
            Failing::new("null mnemonic", Tweak::NullMnemonic, "identity_unreadable"),
            Failing {
                unset: &["MOSTRO_PUBKEY"],
                ..Failing::new("no node key", Tweak::None, "configuration_missing")
            },
            Failing {
                unset: &["RELAYS"],
                ..Failing::new("no relays", Tweak::None, "configuration_missing")
            },
            Failing {
                set: &[("MOSTRO_PUBKEY", SENTINEL)],
                ..Failing::new("bad node key", Tweak::None, "configuration_invalid")
            },
            Failing {
                set: &[("RELAYS", "http://SENTINEL-zq81.invalid")],
                ..Failing::new("bad relay", Tweak::None, "configuration_invalid")
            },
        ];
        for case in &cases {
            check_failure(case).await;
        }
    }

    #[tokio::test]
    async fn an_unreachable_relay_is_a_connection_failure_and_a_silent_one_a_timeout() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.files();
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);

        let run = room
            .run_env(
                &["--output", "json", "message", "list"],
                &[("RELAYS", dead.as_str())],
                &[],
            )
            .await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let outcome = one_document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["error"]["code"], "relay_connection_failed");
        assert_eq!(outcome["meta"]["snapshot"], Value::Null);
        assert!(!run.stdout_text().contains(&dead), "the relay was echoed");

        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let run = room
            .run_env(
                &["--output", "json", "message", "list"],
                &[("RELAYS", silent.url.as_str())],
                &[],
            )
            .await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let outcome = one_document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["error"]["code"], "read_timeout");
        assert!(outcome["meta"]["snapshot"].is_object());
        assert_eq!(room.files(), before);
    }

    #[tokio::test]
    async fn streams_that_cannot_be_written_never_change_the_result_or_the_store() {
        let room = Mailroom::with(3, 1, flow).await;
        let before = room.files();

        // stdout closed: the document cannot be shown; exit 1, the line on stderr.
        for args in [
            &["message", "list"][..],
            &["--output", "json", "message", "list"],
        ] {
            let mut command = room.command(args);
            command.stdout(closed_pipe());
            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
            assert_eq!(run.code(), Some(1), "{args:?}: {}", run.stderr_text());
            assert!(run.stdout.is_empty());
            assert!(
                !run.stderr_text().contains(BOND_INVOICE),
                "the invoice went to stderr"
            );
        }

        // stderr closed: an error keeps its exit status, a success is untouched.
        let mut command = room.command(&["message", "list", "--since", "0"]);
        command.stderr(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(2));
        let mut command = room.command(&["--output", "json", "--verbose", "message", "list"]);
        command.stderr(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(0));
        assert_eq!(one_document(&run)["status"], "ok");
        assert_eq!(room.files(), before);
    }

    #[tokio::test]
    async fn text_from_the_node_cannot_reach_the_terminal_raw() {
        let room = Mailroom::with(3, 1, |node, keys, _| async move {
            let to = keys.iter().find(|(i, _)| *i == 2).unwrap().1.public_key();
            vec![
                node_event(
                    &node,
                    to,
                    &mail_text(id(1), "pwned\u{1b}]0;title\u{7}\u{1b}[31m"),
                    minutes_ago(3),
                )
                .await,
            ]
        })
        .await;

        let run = room.run(&["message", "list"]).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let text = run.stdout_text();
        assert!(text.contains("pwned"), "{text:?}");
        assert!(
            !text.contains('\u{1b}') && !text.contains('\u{7}'),
            "{text:?}"
        );
    }

    #[tokio::test]
    async fn work_and_transport_settings_do_not_concern_a_read() {
        let room = Mailroom::with(3, 1, flow).await;

        for (front, set) in [
            (&["--pow", SENTINEL][..], &[][..]),
            (&["--transport", SENTINEL][..], &[][..]),
            (&[][..], &[("POW", SENTINEL), ("TRANSPORT", SENTINEL)][..]),
        ] {
            let mut args = vec!["--output", "json"];
            args.extend(front);
            args.extend(["message", "list"]);
            let run = room.run_env(&args, set, &[]).await;
            assert_eq!(run.code(), Some(0), "{args:?}: {}", run.stderr_text());
            assert_eq!(one_document(&run)["status"], "ok");
        }
    }
}
