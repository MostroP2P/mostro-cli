//! A scripted fake Mostro daemon and identity-store seeding for the order-info
//! tests. Included with `#[path]` next to `harness.rs`; each test crate uses a
//! different subset.
//!
//! The daemon is a normal Nostr client holding generated node keys. It
//! subscribes to the kind-14 messages addressed to the node on a loopback
//! relay, decrypts each request with `unwrap_incoming`, records it, and
//! publishes whatever its script returns. It never contacts anything else.
#![allow(dead_code)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::SmallOrder;
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, Payload, Transport,
    WrapOptions, NOSTR_INFO_EVENT_KIND,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, EventBuilder, EventId, Filter, FinalizeEvent, Keys, Kind,
    PublicKey, Tag,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use uuid::Uuid;

/// One request the fake daemon decrypted.
#[derive(Clone, Debug)]
pub struct Received {
    pub event_id: EventId,
    /// The event author.
    pub author: PublicKey,
    /// The identity proven inside the message.
    pub identity: PublicKey,
    pub request_id: Option<u64>,
    pub action: Action,
    /// The ids of an `Ids` payload, empty for anything else.
    pub ids: Vec<Uuid>,
}

/// Who signs an outgoing message.
#[derive(Clone, Debug)]
pub enum Sender {
    /// The fake daemon's own keys.
    Node,
    /// Another key, to exercise the wrong-author rule.
    Other(Keys),
}

/// Who a message is addressed to.
#[derive(Clone, Debug)]
pub enum Recipient {
    /// The identity that sent the request.
    Requester,
    /// Another key, to exercise unrelated traffic.
    Other(PublicKey),
}

/// One message the daemon publishes in answer to a request.
#[derive(Clone, Debug)]
pub struct Outgoing {
    pub message: Message,
    pub from: Sender,
    pub to: Recipient,
    pub delay: Duration,
}

impl Outgoing {
    /// A message from the node to the requester, sent at once.
    pub fn to_requester(message: Message) -> Self {
        Self {
            message,
            from: Sender::Node,
            to: Recipient::Requester,
            delay: Duration::ZERO,
        }
    }

    pub fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn signed_by(mut self, keys: Keys) -> Self {
        self.from = Sender::Other(keys);
        self
    }

    pub fn addressed_to(mut self, key: PublicKey) -> Self {
        self.to = Recipient::Other(key);
        self
    }
}

/// An `orders` reply to `request`.
pub fn orders_reply(request: &Received, orders: Vec<SmallOrder>) -> Message {
    Message::new_order(
        None,
        request.request_id,
        None,
        Action::Orders,
        Some(Payload::Orders(orders)),
    )
}

/// A refusal of `request` with `reason`.
pub fn refusal(request: &Received, reason: CantDoReason) -> Message {
    Message::cant_do(
        None,
        request.request_id,
        Some(Payload::CantDo(Some(reason))),
    )
}

/// A reply to `request` that echoes the id but carries the wrong payload.
pub fn malformed_reply(request: &Received) -> Message {
    Message::new_order(
        None,
        request.request_id,
        None,
        Action::Orders,
        Some(Payload::TextMessage("not orders".to_string())),
    )
}

/// What the daemon sends for each request it receives.
pub type Respond = Arc<dyn Fn(&Received) -> Vec<Outgoing> + Send + Sync>;

/// A script that never answers.
pub fn silent() -> Respond {
    Arc::new(|_| Vec::new())
}

/// A script that answers every request with the same `orders` reply.
pub fn always_orders(orders: Vec<SmallOrder>) -> Respond {
    Arc::new(move |request| {
        vec![Outgoing::to_requester(orders_reply(
            request,
            orders.clone(),
        ))]
    })
}

/// The running fake daemon. Dropping it stops it.
pub struct FakeMostro {
    pub keys: Keys,
    received: Arc<Mutex<Vec<Received>>>,
    task: JoinHandle<()>,
}

impl FakeMostro {
    /// Connect to `relay_url` as the node and start answering.
    pub async fn start(relay_url: &str, keys: Keys, respond: Respond) -> Self {
        let client = Client::default();
        client.add_relay(relay_url).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        let mut notifications = client.notifications();
        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .pubkey(keys.public_key())
            .limit(0);
        client.subscribe(filter).await.unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let task = {
            let (keys, received) = (keys.clone(), Arc::clone(&received));
            tokio::spawn(async move {
                while let Some(notification) = notifications.next().await {
                    let ClientNotification::Event { event, .. } = notification else {
                        continue;
                    };
                    if event.kind != Kind::PrivateDirectMessage {
                        continue;
                    }
                    let Ok(Some(request)) = unwrap_incoming(&event, &keys).await else {
                        continue;
                    };
                    let inner = request.message.get_inner_message_kind();
                    let ids = match &inner.payload {
                        Some(Payload::Ids(ids)) => ids.clone(),
                        _ => Vec::new(),
                    };
                    let seen = Received {
                        event_id: event.id,
                        author: request.sender,
                        identity: request.identity,
                        request_id: inner.request_id,
                        action: inner.action.clone(),
                        ids,
                    };
                    received.lock().unwrap().push(seen.clone());
                    for outgoing in respond(&seen) {
                        publish(&client, &keys, &seen, outgoing).await;
                    }
                }
            })
        };
        Self {
            keys,
            received,
            task,
        }
    }

    /// The requests received so far, oldest first.
    pub fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    pub fn pubkey(&self) -> PublicKey {
        self.keys.public_key()
    }
}

impl Drop for FakeMostro {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn publish(client: &Client, node: &Keys, request: &Received, outgoing: Outgoing) {
    if !outgoing.delay.is_zero() {
        sleep(outgoing.delay).await;
    }
    let signer = match &outgoing.from {
        Sender::Node => node.clone(),
        Sender::Other(keys) => keys.clone(),
    };
    let receiver = match outgoing.to {
        Recipient::Requester => request.author,
        Recipient::Other(key) => key,
    };
    let event = wrap_message_with(
        Transport::Nip44Direct,
        &outgoing.message,
        &signer,
        &signer,
        receiver,
        WrapOptions::default(),
    )
    .await
    .unwrap();
    client.send_event(&event).await.unwrap();
}

/// The node's kind-38385 info event, with only the tags given.
pub fn info_event(
    node: &Keys,
    protocol_version: Option<u8>,
    pow: Option<u8>,
    pow_first_contact: Option<u8>,
    max_orders_per_response: Option<u32>,
) -> Event {
    let mut tags = vec![Tag::custom("d", [node.public_key().to_hex()])];
    let mut add = |name: &str, value: Option<String>| {
        if let Some(value) = value {
            tags.push(Tag::custom(name, [value]));
        }
    };
    add("protocol_version", protocol_version.map(|v| v.to_string()));
    add("pow", pow.map(|v| v.to_string()));
    add(
        "pow_first_contact",
        pow_first_contact.map(|v| v.to_string()),
    );
    add(
        "max_orders_per_response",
        max_orders_per_response.map(|v| v.to_string()),
    );
    EventBuilder::new(Kind::Custom(NOSTR_INFO_EVENT_KIND), "")
        .tags(tags)
        .finalize(node)
        .unwrap()
}

/// Create `dir/mcli.db` the way the CLI does (same tables, a generated
/// mnemonic, the real `User` code) and return the identity key it derives.
/// `dir` must exist.
pub async fn seed_identity(dir: &Path) -> Keys {
    let path = dir.join("mcli.db");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS orders (
            id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL,
            amount INTEGER NOT NULL, min_amount INTEGER, max_amount INTEGER,
            fiat_code TEXT NOT NULL, fiat_amount INTEGER NOT NULL,
            payment_method TEXT NOT NULL, premium INTEGER NOT NULL,
            trade_keys TEXT, counterparty_pubkey TEXT, is_mine BOOLEAN,
            buyer_invoice TEXT, request_id INTEGER, created_at INTEGER,
            expires_at INTEGER, solver_pubkey TEXT
        );
        CREATE TABLE IF NOT EXISTS users (
            i0_pubkey char(64) PRIMARY KEY, mnemonic TEXT,
            last_trade_index INTEGER, created_at INTEGER
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
        .unwrap()
        .to_string();
    mostro_client::db::User::new(mnemonic, &pool).await.unwrap();
    let keys = mostro_client::db::User::get_identity_keys(&pool)
        .await
        .unwrap();
    pool.close().await;
    keys
}
