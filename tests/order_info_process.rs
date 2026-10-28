//! Process and exchange tests for `order info` (spec 002).
//!
//! Slice 1 holds only the self-tests of the shared fixtures: the scripted fake
//! daemon on the loopback relay and the identity-store seeding. The command's
//! own scenarios are added with the slices that implement it.

use std::fs;
use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, Message, Payload, Transport, WrapOptions,
};
use nostr_sdk::prelude::{Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay};
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    always_orders, info_event, orders_reply, seed_identity, silent, FakeMostro, Outgoing, Respond,
};
use harness::{is_empty_dir, Backend, LoopbackRelay, Sandbox};

const REQUEST_ID: u64 = 8721;

fn order(id: Uuid) -> SmallOrder {
    SmallOrder::new(
        Some(id),
        Some(OrderKind::Sell),
        Some(Status::Active),
        15_000,
        "USD".to_string(),
        None,
        None,
        15,
        "bank transfer".to_string(),
        1,
        None,
        None,
        None,
        Some(1_792_108_800),
        Some(1_792_195_200),
    )
}

/// A loopback relay holding the node's info event, and the fake daemon on it.
async fn daemon(respond: Respond) -> (LoopbackRelay, FakeMostro) {
    let node = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, node, respond).await;
    (relay, fake)
}

/// A requester client with the notification stream armed before it sends.
struct Requester {
    client: Client,
    keys: Keys,
    node: Keys,
}

impl Requester {
    async fn connect(relay: &LoopbackRelay, node: &Keys) -> Self {
        let keys = Keys::generate();
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        Self {
            client,
            keys,
            node: node.clone(),
        }
    }

    /// Subscribe to messages for this key, publish the request, and collect
    /// the decoded replies seen within `wait`.
    async fn ask(&self, ids: Vec<Uuid>, wait: Duration) -> Vec<(Option<u64>, Action)> {
        let mut notifications = self.client.notifications();
        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .pubkey(self.keys.public_key())
            .limit(0);
        self.client.subscribe(filter).await.unwrap();
        let request = Message::new_order(
            None,
            Some(REQUEST_ID),
            None,
            Action::Orders,
            Some(Payload::Ids(ids)),
        );
        let event: Event = wrap_message_with(
            Transport::Nip44Direct,
            &request,
            &self.keys,
            &self.keys,
            self.node.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();
        let sent = event.id;
        self.client.send_event(&event).await.unwrap();

        let mut replies = Vec::new();
        let _ = timeout(wait, async {
            while let Some(notification) = notifications.next().await {
                let ClientNotification::Event { event, .. } = notification else {
                    continue;
                };
                if event.kind != Kind::PrivateDirectMessage || event.id == sent {
                    continue;
                }
                if let Ok(Some(seen)) = unwrap_incoming(&event, &self.keys).await {
                    assert_eq!(seen.sender, self.node.public_key());
                    let inner = seen.message.get_inner_message_kind();
                    replies.push((inner.request_id, inner.action.clone()));
                }
            }
        })
        .await;
        replies
    }
}

#[tokio::test]
async fn the_fake_daemon_records_a_request_and_replies_by_script() {
    let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let (relay, fake) = daemon(always_orders(vec![order(ids[0])])).await;
    let requester = Requester::connect(&relay, &fake.keys).await;

    let replies = requester.ask(ids.clone(), Duration::from_millis(800)).await;

    assert_eq!(replies, vec![(Some(REQUEST_ID), Action::Orders)]);
    let seen = fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].request_id, Some(REQUEST_ID));
    assert_eq!(seen[0].action, Action::Orders);
    assert_eq!(seen[0].ids, ids);
    assert_eq!(seen[0].author, requester.keys.public_key());
    assert_eq!(seen[0].identity, requester.keys.public_key());
}

#[tokio::test]
async fn the_fake_daemon_scripts_noise_before_the_reply_and_silence() {
    let noisy: Respond = std::sync::Arc::new(|request| {
        let other = Message::new_order(
            None,
            Some(request.request_id.unwrap() + 1),
            None,
            Action::Orders,
            None,
        );
        vec![
            Outgoing::to_requester(other),
            Outgoing::to_requester(orders_reply(request, Vec::new()))
                .after(Duration::from_millis(50)),
        ]
    });
    let (relay, fake) = daemon(noisy).await;
    let requester = Requester::connect(&relay, &fake.keys).await;
    let replies = requester
        .ask(vec![Uuid::new_v4()], Duration::from_millis(800))
        .await;
    assert_eq!(
        replies,
        vec![
            (Some(REQUEST_ID + 1), Action::Orders),
            (Some(REQUEST_ID), Action::Orders)
        ]
    );

    let (relay, fake) = daemon(silent()).await;
    let requester = Requester::connect(&relay, &fake.keys).await;
    let replies = requester
        .ask(vec![Uuid::new_v4()], Duration::from_millis(400))
        .await;
    assert!(replies.is_empty());
    assert_eq!(fake.received().len(), 1, "the request still arrived");
}

#[tokio::test]
async fn a_seeded_identity_is_a_real_store_with_the_production_derived_key() {
    let sandbox = Sandbox::new().unwrap();
    let keys = seed_identity(&sandbox.mcli_dir()).await;

    let path = sandbox.mcli_dir().join("mcli.db");
    let before = fs::read(&path).unwrap();
    assert!(before.len() > 1_000, "an empty file is not a database");
    assert!(!is_empty_dir(&sandbox.mcli_dir()));
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));

    // The production reader derives the same key from the file.
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", path.display()))
        .await
        .unwrap();
    let derived = mostro_client::db::User::get_identity_keys(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert_eq!(derived.public_key(), keys.public_key());
    assert_eq!(fs::read(&path).unwrap(), before, "reading changed the file");
}
