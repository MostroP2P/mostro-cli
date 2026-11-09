//! Tests for `order chat` (spec 019): the counterparty column of the store and
//! the lookup of the counterparty in the node's messages (unit 1), the chat
//! read and send (unit 2), the command in human mode (unit 3) and its JSON
//! output (unit 4).
//!
//! Every store is an isolated sandbox seeded the way the CLI creates one; the
//! node's mail and the chat sit on in-process loopback relays, signed by
//! generated keys. Nothing else is contacted.

use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use mostro_client::msg::chat::{read_chat, send_chat, ChatLine, SendFailure};
use mostro_client::msg::fetch_msg;
use mostro_client::outcome::{Publication, Warnings};
use mostro_core::prelude::{chat_filter, derive_chat_keys, wrap_chat_message, Action, Message};
use nostr_sdk::prelude::{
    Event, Filter, Keys, Kind, LocalRelay, LocalRelayBuilderNip42, MachineReadablePrefix,
    PublicKey, Timestamp, WritePolicy, WritePolicyResult,
};
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    mail_fiat_sent_ok, mail_trade_keys, mail_waiting_seller, node_event, seed_identity, Traded,
};
use harness::{Backend, LoopbackRelay, Sandbox};
use mostro_client::commands::order::counterparty::find_counterparty;
use mostro_client::msg::{PublicReadConfig, ReadLimits};
use mostro_client::outcome::ErrorCode;
use mostro_client::store::{Role, Store, StoreError, Trade};

const ORDER: Uuid = Uuid::from_u128(0x0c4a7);
const OTHER: Uuid = Uuid::from_u128(0x07e4);

fn limits() -> ReadLimits {
    ReadLimits {
        connect: Duration::from_secs(2),
        read: Duration::from_millis(1_500),
        cleanup: Duration::from_millis(500),
        ..ReadLimits::default()
    }
}

fn days_ago(days: u64) -> Timestamp {
    Timestamp::from(Timestamp::now().as_secs() - days * 86_400)
}

/// A relay URL nothing listens on.
fn dead_relay() -> String {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", free.local_addr().unwrap());
    drop(free);
    url
}

/// Rejects every write with a machine-readable refusal.
#[derive(Debug)]
struct BlockWrites;

impl WritePolicy for BlockWrites {
    fn admit_event<'a>(
        &'a self,
        _event: &'a Event,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = WritePolicyResult> + Send + 'a>> {
        Box::pin(async {
            WritePolicyResult::reject(MachineReadablePrefix::Blocked, "writes are off")
        })
    }
}

/// A user who took `ORDER` (and `OTHER`) as the buyer with `trade`; `them` is
/// the seller.
struct Room {
    sandbox: Sandbox,
    store: Store,
    node: Keys,
    trade: Keys,
    them: Keys,
}

impl Room {
    async fn new() -> Self {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let (node, trade, them) = (Keys::generate(), Keys::generate(), Keys::generate());
        for (index, id) in [(1, ORDER), (2, OTHER)] {
            let small = Traded::active(id, them.public_key(), trade.public_key()).small();
            let keys = Trade {
                index,
                keys: if id == ORDER {
                    trade.clone()
                } else {
                    Keys::generate()
                },
            };
            store
                .save_order_as(&small, &keys, 1, Role::Taker)
                .await
                .unwrap();
        }
        Self {
            sandbox,
            store,
            node,
            trade,
            them,
        }
    }

    fn db(&self) -> std::path::PathBuf {
        self.sandbox.mcli_dir().join("mcli.db")
    }

    fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.db()).unwrap()
    }

    /// Run `statement` with `value` bound to `?1` and `ORDER` to `?2`.
    async fn sql(&self, statement: &'static str, value: Option<&str>) {
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(self.db());
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(statement)
            .bind(value)
            .bind(ORDER.to_string())
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    /// Make every update of an order row fail from now on.
    async fn freeze(&self) {
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(self.db());
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    /// Every order row and the user row as JSON, read without writing.
    async fn rows(&self) -> Vec<Value> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(self.db())
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT json_object('id', id, 'kind', kind, 'status', status, 'amount', amount, \
             'min', min_amount, 'max', max_amount, 'fiat', fiat_code, 'fiat_amount', fiat_amount, \
             'method', payment_method, 'premium', premium, 'trade_keys', trade_keys, \
             'counterparty_pubkey', counterparty_pubkey, 'is_mine', is_mine, \
             'invoice', buyer_invoice, 'request_id', request_id, 'created_at', created_at, \
             'expires_at', expires_at, 'solver', solver_pubkey) FROM orders \
             UNION ALL SELECT json_object('user', i0_pubkey, 'mnemonic', mnemonic, \
             'last', last_trade_index, 'created_at', created_at) FROM users",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        pool.close().await;
        rows.iter()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect()
    }

    async fn column(&self, order: Uuid) -> Option<String> {
        self.rows()
            .await
            .into_iter()
            .find(|row| row["id"] == order.to_string())
            .and_then(|row| row["counterparty_pubkey"].as_str().map(str::to_string))
    }

    /// A loopback relay holding `events`.
    async fn relay(&self, events: Vec<Event>) -> LoopbackRelay {
        let relay = LocalRelay::new();
        for event in events {
            relay.add_event(event).await.unwrap();
        }
        LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap()
    }

    /// The node's `message` to our trade key, `days` old.
    async fn mail(&self, message: &Message, days: u64) -> Event {
        node_event(&self.node, self.trade.public_key(), message, days_ago(days)).await
    }

    fn config(&self, url: &str) -> PublicReadConfig {
        PublicReadConfig::new(self.node.public_key(), url).unwrap()
    }

    async fn find(&self, url: &str) -> Result<PublicKey, (ErrorCode, bool)> {
        find_counterparty(
            ORDER,
            &self.trade,
            &self.config(url),
            &self.store,
            &limits(),
            Timestamp::now(),
        )
        .await
        .map(|found| {
            assert!(found.warnings().is_empty());
            found.key()
        })
        .map_err(|failure| (failure.code(), failure.warnings().is_empty()))
    }
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

mod store {
    use super::*;

    #[tokio::test]
    async fn a_saved_counterparty_is_read_back_and_only_that_column_changes() {
        let room = Room::new().await;
        let before = room.rows().await;
        let them = room.them.public_key();

        room.store.set_counterparty(ORDER, &them).await.unwrap();

        assert_eq!(room.store.counterparty(ORDER).await, Ok(Some(them)));
        assert_eq!(room.store.counterparty(OTHER).await, Ok(None));
        assert_eq!(
            room.column(ORDER).await,
            Some(them.to_hex()),
            "canonical hex"
        );
        let mut after = room.rows().await;
        let changed = after
            .iter_mut()
            .find(|row| row["id"] == ORDER.to_string())
            .unwrap();
        changed["counterparty_pubkey"] = Value::Null;
        assert_eq!(after, before, "every other column and row is unchanged");

        let unknown = Uuid::from_u128(0xdead);
        assert_eq!(
            room.store.set_counterparty(unknown, &them).await,
            Err(StoreError::Unwritable),
            "no row for that order"
        );
    }

    #[tokio::test]
    async fn an_empty_or_unparsable_column_reads_as_none_without_creating_files() {
        let room = Room::new().await;
        let dir = room.sandbox.mcli_dir();
        let valid = room.them.public_key().to_hex();
        for (value, expected) in [
            (None, None),
            (Some(""), None),
            (Some("not-a-key"), None),
            (Some("abcd"), None),
            (Some(valid.as_str()), Some(room.them.public_key())),
        ] {
            room.sql(
                "UPDATE orders SET counterparty_pubkey = ?1 WHERE id = ?2",
                value,
            )
            .await;
            let (files, bytes) = (file_names(&dir), room.bytes());

            let read = room.store.counterparty(ORDER).await;

            assert_eq!(read, Ok(expected), "{value:?}");
            assert_eq!(file_names(&dir), files, "no file created beside the store");
            assert_eq!(room.bytes(), bytes, "nothing written");
        }
        assert_eq!(
            room.store.counterparty(Uuid::from_u128(0xdead)).await,
            Ok(None),
            "an unknown order"
        );
        std::fs::remove_file(room.db()).unwrap();
        assert_eq!(
            room.store.counterparty(ORDER).await,
            Err(StoreError::Missing)
        );
        assert!(!room.db().exists(), "a missing store is not created");
    }
}

mod counterparty {
    use super::*;

    #[tokio::test]
    async fn a_stored_counterparty_is_used_without_contacting_any_relay() {
        let room = Room::new().await;
        let them = room.them.public_key();
        room.store.set_counterparty(ORDER, &them).await.unwrap();
        let bytes = room.bytes();

        assert_eq!(room.find(&dead_relay()).await, Ok(them), "with no warning");

        assert_eq!(room.bytes(), bytes, "nothing written");
    }

    #[tokio::test]
    async fn store_failures_keep_their_codes_and_contact_no_relay() {
        let room = Room::new().await;
        let relay = LoopbackRelay::start(Backend::Silent).await.unwrap();
        std::fs::remove_file(room.db()).unwrap();

        assert_eq!(
            room.find(&relay.url).await,
            Err((ErrorCode::IdentityMissing, true))
        );
        assert!(!room.db().exists(), "a missing store is not created");
        std::fs::create_dir(room.db()).unwrap();
        assert_eq!(
            room.find(&relay.url).await,
            Err((ErrorCode::StoreUnreadable, true))
        );

        assert_eq!(relay.connections(), 0, "no relay was contacted");
    }

    #[tokio::test]
    async fn our_own_key_in_the_column_is_not_the_counterparty() {
        let room = Room::new().await;
        room.store
            .set_counterparty(ORDER, &room.trade.public_key())
            .await
            .unwrap();
        let relay = room.relay(Vec::new()).await;

        assert_eq!(
            room.find(&relay.url).await,
            Err((ErrorCode::CounterpartyUnknown, true))
        );
        assert!(relay.connections() > 0, "the node's mail was read");
    }

    #[tokio::test]
    async fn it_is_learned_from_eight_day_old_node_mail_and_saved() {
        let room = Room::new().await;
        let (ours, them) = (room.trade.public_key(), room.them.public_key());
        let accepted = mail_trade_keys(Action::HoldInvoicePaymentAccepted, ORDER, ours, them);
        let relay = room.relay(vec![room.mail(&accepted, 8).await]).await;

        assert_eq!(room.find(&relay.url).await, Ok(them));

        assert_eq!(room.store.counterparty(ORDER).await, Ok(Some(them)));
        let expired = room.relay(Vec::new()).await;
        assert_eq!(
            room.find(&expired.url).await,
            Ok(them),
            "the row keeps it once the node's mail has expired"
        );
    }

    #[tokio::test]
    async fn it_is_learned_from_fiat_sent_ok() {
        let room = Room::new().await;
        let them = room.them.public_key();
        let relay = room
            .relay(vec![
                room.mail(&mail_waiting_seller(ORDER), 2).await,
                room.mail(&mail_fiat_sent_ok(ORDER, them), 1).await,
            ])
            .await;

        assert_eq!(room.find(&relay.url).await, Ok(them));
        assert_eq!(room.column(ORDER).await, Some(them.to_hex()));
    }

    #[tokio::test]
    async fn before_the_escrow_is_locked_it_is_counterparty_unknown() {
        let room = Room::new().await;
        let ours = room.trade.public_key();
        let stale = Keys::generate().public_key();
        let relay = room
            .relay(vec![
                room.mail(
                    &mail_trade_keys(Action::WaitingSellerToPay, ORDER, ours, stale),
                    3,
                )
                .await,
                room.mail(&mail_trade_keys(Action::NewOrder, ORDER, ours, stale), 2)
                    .await,
                room.mail(
                    &mail_trade_keys(Action::BuyerTookOrder, OTHER, ours, stale),
                    1,
                )
                .await,
            ])
            .await;
        let bytes = room.bytes();

        assert_eq!(
            room.find(&relay.url).await,
            Err((ErrorCode::CounterpartyUnknown, true))
        );
        assert_eq!(room.bytes(), bytes, "the store is unchanged");
    }

    #[tokio::test]
    async fn a_store_that_refuses_the_save_still_returns_the_key() {
        let room = Room::new().await;
        let (ours, them) = (room.trade.public_key(), room.them.public_key());
        let took = mail_trade_keys(Action::BuyerTookOrder, ORDER, ours, them);
        let relay = room.relay(vec![room.mail(&took, 1).await]).await;
        room.freeze().await;

        assert_eq!(room.find(&relay.url).await, Ok(them));
        assert_eq!(room.column(ORDER).await, None, "the save was refused");
    }

    #[tokio::test]
    async fn relay_failures_keep_their_codes() {
        let room = Room::new().await;
        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let bytes = room.bytes();

        assert_eq!(
            room.find(&silent.url).await.map_err(|(code, _)| code),
            Err(ErrorCode::ReadTimeout)
        );
        assert_eq!(
            room.find(&dead_relay()).await.map_err(|(code, _)| code),
            Err(ErrorCode::RelayConnectionFailed)
        );
        assert_eq!(room.bytes(), bytes, "the store is unchanged");
    }
}

mod transport {
    use super::*;

    /// Our trade key and the counterparty's, and relays at `url`.
    struct Chat {
        me: Keys,
        them: Keys,
        config: PublicReadConfig,
    }

    impl Chat {
        fn new(url: &str) -> Self {
            let config = PublicReadConfig::new(Keys::generate().public_key(), url).unwrap();
            let (me, them) = (Keys::generate(), Keys::generate());
            Self { me, them, config }
        }

        async fn send(&self, text: &str) -> Result<(ChatLine, Warnings), SendFailure> {
            let them = self.them.public_key();
            send_chat(&self.config, &self.me, &them, text, &limits()).await
        }

        /// The last hour of the conversation as `reader` sees it with `peer`.
        async fn read(&self, reader: &Keys, peer: &Keys) -> (Vec<ChatLine>, bool) {
            let now = Timestamp::now();
            let since = Timestamp::from(now.as_secs() - 3_600);
            let peer = peer.public_key();
            let read = read_chat(&self.config, reader, &peer, since, now, &limits())
                .await
                .unwrap();
            assert!(read.warnings.is_empty());
            (read.lines, read.truncated)
        }
    }

    async fn serve(backend: Backend) -> LoopbackRelay {
        LoopbackRelay::start(backend).await.unwrap()
    }

    /// The code and publication of a failed send, and the text of its line.
    fn failure(
        sent: Result<(ChatLine, Warnings), SendFailure>,
    ) -> (ErrorCode, Publication, Option<String>) {
        let failure = sent.unwrap_err();
        let text = failure.line().map(|line| line.text.clone());
        (failure.code(), failure.publication(), text)
    }

    #[tokio::test]
    async fn the_conversation_is_one_bounded_read_from_both_sides() {
        let local = LocalRelay::new();
        let relay = serve(Backend::Relay(Box::new(local.clone()))).await;
        let chat = Chat::new(&relay.url);
        let stranger = Keys::generate();
        let ours = derive_chat_keys(&chat.me, &chat.them.public_key()).unwrap();
        let other = derive_chat_keys(&chat.me, &stranger.public_key()).unwrap();
        for (writer, (conv, sign), text) in [
            (&chat.me, &ours, "fiat sent"),
            (&chat.them, &ours, "got it"),
            (&stranger, &ours, "intruder"),
            (&chat.me, &other, "another chat"),
        ] {
            let event = wrap_chat_message(writer, conv, sign, text).await.unwrap();
            local.add_event(event).await.unwrap();
        }

        let (lines, truncated) = chat.read(&chat.me, &chat.them).await;

        let mut written: Vec<(&str, PublicKey)> = lines
            .iter()
            .map(|line| (line.text.as_str(), line.sender))
            .collect();
        written.sort_unstable();
        assert_eq!(
            written,
            [
                ("fiat sent", chat.me.public_key()),
                ("got it", chat.them.public_key())
            ]
        );
        let order = |line: &ChatLine| (line.created_at, line.message_id);
        assert!(lines
            .windows(2)
            .all(|pair| order(&pair[0]) < order(&pair[1])));
        assert!(!truncated);
        assert_eq!(relay.connections(), 1, "one read");
        assert_eq!(chat.read(&chat.them, &chat.me).await.0, lines, "both ways");
    }

    #[tokio::test]
    async fn a_sent_message_is_one_kind_14_event_the_counterparty_opens() {
        let relay = serve(Backend::Relay(Box::new(LocalRelay::new()))).await;
        let chat = Chat::new(&relay.url);

        let (line, warnings) = chat.send("on my way\nsee you").await.unwrap();

        assert!(warnings.is_empty());
        assert_eq!(line.text, "on my way\nsee you");
        assert_eq!(
            line.sender,
            chat.me.public_key(),
            "signed inside by our trade key"
        );
        assert_eq!(chat.read(&chat.them, &chat.me).await.0, vec![line]);
        let everything = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .since(Timestamp::from(0))
            .limit(10);
        let stored = fetch_msg(&chat.config, everything, &limits())
            .await
            .unwrap();
        assert_eq!(stored.events().len(), 1, "one event");
        let event = stored.events().first().unwrap();
        let (conv, sign) = derive_chat_keys(&chat.them, &chat.me.public_key()).unwrap();
        assert_eq!(event.pubkey, sign.public_key(), "authored by K_sign");
        let tagged: Vec<PublicKey> = event.tags.public_keys().collect();
        assert_eq!(tagged, [conv.public_key()]);
        assert_eq!(event.tags.len(), 1, "one p tag, no nonce");
    }

    #[tokio::test]
    async fn relays_that_refuse_it_mean_nothing_was_published() {
        let refusing = LocalRelay::builder().write_policy(BlockWrites).build();
        let relay = serve(Backend::Relay(Box::new(refusing))).await;

        let refused = Chat::new(&relay.url).send("hello").await;

        let expected = (ErrorCode::RequestRejected, Publication::NotAttempted, None);
        assert_eq!(failure(refused), expected);
    }

    #[tokio::test]
    async fn one_confirmation_is_enough_and_a_relay_asking_for_auth_still_gets_it() {
        // `auth` answers the first EVENT with `auth-required:` and stores the message only when it
        // is sent again after the AUTH, so after `open` confirmed. NIP-42 checks the relay URL.
        let refusing = LocalRelay::builder().write_policy(BlockWrites).build();
        let refusing = serve(Backend::Relay(Box::new(refusing))).await;
        let open = serve(Backend::Relay(Box::new(LocalRelay::new()))).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let auth_url = format!("ws://{}", listener.local_addr().unwrap());
        let auth = LocalRelay::builder()
            .port(listener.local_addr().unwrap().port())
            .nip42(LocalRelayBuilderNip42::write())
            .build();
        let backend = Backend::Relay(Box::new(auth));
        tokio::spawn(harness::accept_loop(listener, backend, Default::default()));
        let chat = Chat::new(&format!("{},{},{auth_url}", refusing.url, open.url));

        let (line, _) = chat.send("hello").await.unwrap();

        let config = PublicReadConfig::new(Keys::generate().public_key(), &auth_url).unwrap();
        let auth = Chat { config, ..chat };
        assert_eq!(auth.read(&auth.them, &auth.me).await.0, vec![line]);
    }

    #[tokio::test]
    async fn a_relay_at_its_limit_of_200_marks_the_read_truncated() {
        let local = LocalRelay::new();
        let relay = serve(Backend::Relay(Box::new(local.clone()))).await;
        let chat = Chat::new(&relay.url);
        let (conv, sign) = derive_chat_keys(&chat.me, &chat.them.public_key()).unwrap();
        for n in 0..201 {
            let event = wrap_chat_message(&chat.me, &conv, &sign, &n.to_string()).await;
            local.add_event(event.unwrap()).await.unwrap();
        }

        let (lines, truncated) = chat.read(&chat.me, &chat.them).await;

        assert_eq!((lines.len(), truncated), (200, true), "the relay's limit");
    }

    #[tokio::test]
    async fn a_relay_that_never_confirms_is_ambiguous() {
        let relay = serve(Backend::Silent).await;

        let unconfirmed = Chat::new(&relay.url).send("hello").await;

        let line = Some("hello".to_string());
        let expected = (ErrorCode::RequestRejected, Publication::Unknown, line);
        assert_eq!(failure(unconfirmed), expected);
    }

    #[tokio::test]
    async fn an_unreachable_relay_fails_before_sending() {
        let unsent = Chat::new(&dead_relay()).send("hello").await;

        let expected = (
            ErrorCode::RelayConnectionFailed,
            Publication::NotAttempted,
            None,
        );
        assert_eq!(failure(unsent), expected);
    }
}

/// Never answers a write: the relay neither stores nor refuses it.
#[derive(Debug)]
struct NeverConfirm;

impl WritePolicy for NeverConfirm {
    fn admit_event<'a>(
        &'a self,
        _event: &'a Event,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = WritePolicyResult> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

impl Room {
    /// A message `writer` wrote in the conversation of `ORDER`.
    async fn line(&self, writer: &Keys, text: &str) -> Event {
        let (conv, sign) = derive_chat_keys(&self.trade, &self.them.public_key()).unwrap();
        wrap_chat_message(writer, &conv, &sign, text).await.unwrap()
    }

    /// How many envelopes of that conversation the relays at `url` hold.
    async fn envelopes(&self, url: &str) -> usize {
        let (_, sign) = derive_chat_keys(&self.trade, &self.them.public_key()).unwrap();
        let filter = chat_filter(sign.public_key())
            .since(Timestamp::from(0))
            .limit(50);
        let fetched = fetch_msg(&self.config(url), filter, &limits()).await;
        fetched.unwrap().events().len()
    }

    /// The counterparty saved in the row, as a previous lookup leaves it.
    async fn store_them(&self) {
        let them = self.them.public_key();
        self.store.set_counterparty(ORDER, &them).await.unwrap();
    }

    /// A loopback relay serving `relay`, holding `events`.
    async fn serve(&self, relay: LocalRelay, events: Vec<Event>) -> LoopbackRelay {
        for event in events {
            relay.add_event(event).await.unwrap();
        }
        LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap()
    }
}

/// The (from, text) of each message of `data`, sorted.
fn written(data: &Value) -> Vec<(String, String)> {
    let mut written: Vec<(String, String)> = data["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            let field = |name: &str| record[name].as_str().unwrap().to_string();
            (field("from"), field("text"))
        })
        .collect();
    written.sort_unstable();
    written
}

fn pair(from: &str, text: &str) -> (String, String) {
    (from.to_string(), text.to_string())
}

mod operation {
    use super::*;
    use chrono::Utc;
    use mostro_client::commands::order::chat::order_chat;
    use mostro_client::outcome::chat::ChatInput;

    const UNKNOWN: Uuid = Uuid::from_u128(0xdead);

    /// `order chat` on `order`, with relays at `url`, as its JSON outcome.
    async fn chat(room: &Room, order: Uuid, message: Option<&str>, url: &str) -> Value {
        let input = ChatInput::validate(order, message, None).unwrap();
        let config = room.config(url);
        let outcome = order_chat(&input, &config, &room.store, &limits(), Utc::now()).await;
        serde_json::to_value(outcome).unwrap()
    }

    /// The status, error code and (publication, acceptance, persistence).
    fn result(outcome: &Value) -> (&str, Option<&str>, [&str; 3]) {
        let meta = &outcome["meta"];
        let stage = |name: &str| meta[name].as_str().unwrap();
        assert_eq!(meta["snapshot"], Value::Null);
        (
            outcome["status"].as_str().unwrap(),
            outcome["error"]["code"].as_str(),
            [
                stage("publication"),
                stage("daemon_acceptance"),
                stage("persistence"),
            ],
        )
    }

    const NOTHING: [&str; 3] = ["not_applicable"; 3];

    #[tokio::test]
    async fn a_read_returns_both_sides_and_learns_the_counterparty() {
        let room = Room::new().await;
        let (ours, them) = (room.trade.public_key(), room.them.public_key());
        let accepted = mail_trade_keys(Action::HoldInvoicePaymentAccepted, ORDER, ours, them);
        let relay = room
            .relay(vec![
                room.mail(&accepted, 1).await,
                room.line(&room.trade, "fiat sent").await,
                room.line(&room.them, "got it").await,
            ])
            .await;

        let outcome = chat(&room, ORDER, None, &relay.url).await;

        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        let data = &outcome["data"];
        assert_eq!(data["order_id"], ORDER.to_string());
        assert_eq!(data["counterparty_pubkey"], them.to_hex());
        let expected = [pair("counterparty", "got it"), pair("me", "fiat sent")];
        assert_eq!(written(data), expected);
        assert_eq!(
            (&data["truncated"], &data["sent"]),
            (&json!(false), &Value::Null)
        );
        assert_eq!(room.column(ORDER).await, Some(them.to_hex()), "cached");
    }

    #[tokio::test]
    async fn an_unknown_order_or_counterparty_fails_before_any_chat_read() {
        let room = Room::new().await;
        let relay = room
            .relay(vec![room.mail(&mail_waiting_seller(ORDER), 1).await])
            .await;
        let failed = |code| ("error", Some(code), NOTHING);
        for message in [None, Some("hello")] {
            let outcome = chat(&room, UNKNOWN, message, &relay.url).await;
            assert_eq!(result(&outcome), failed("order_unknown"), "{message:?}");
        }
        assert_eq!(relay.connections(), 0, "an unknown order reads nothing");

        for message in [None, Some("hello")] {
            let outcome = chat(&room, ORDER, message, &relay.url).await;
            assert_eq!(result(&outcome), failed("counterparty_unknown"));
            assert_eq!(outcome["data"], Value::Null);
        }

        assert_eq!(room.envelopes(&relay.url).await, 0, "nothing was sent");
        std::fs::remove_file(room.db()).unwrap();
        let outcome = chat(&room, ORDER, None, &relay.url).await;
        assert_eq!(result(&outcome), failed("identity_missing"));
        std::fs::create_dir(room.db()).unwrap();
        let outcome = chat(&room, ORDER, None, &relay.url).await;
        assert_eq!(result(&outcome), failed("store_unreadable"));
    }

    #[tokio::test]
    async fn sending_publishes_and_keeps_sent_apart_from_messages() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room
            .relay(vec![room.line(&room.them, "send the fiat").await])
            .await;

        let outcome = chat(&room, ORDER, Some("  sent it \n"), &relay.url).await;

        let published = ["published", "not_applicable", "not_applicable"];
        assert_eq!(result(&outcome), ("ok", None, published));
        let data = &outcome["data"];
        assert_eq!(written(data), [pair("counterparty", "send the fiat")]);
        let sent = &data["sent"];
        assert_eq!(
            (&sent["from"], &sent["text"]),
            (&json!("me"), &json!("sent it"))
        );
        let again = chat(&room, ORDER, None, &relay.url).await;
        let ids: Vec<&Value> = again["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| &record["message_id"])
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&sent["message_id"]), "read back by its id");
        assert_eq!(again["data"]["sent"], Value::Null);
        assert_eq!(room.envelopes(&relay.url).await, 2, "sent once");
    }

    #[tokio::test]
    async fn refused_or_unconfirmed_sends_are_error_or_uncertain() {
        let room = Room::new().await;
        room.store_them().await;
        let said = vec![room.line(&room.them, "hi").await];
        let refusing = LocalRelay::builder().write_policy(BlockWrites).build();
        let refusing = room.serve(refusing, said.clone()).await;

        let refused = chat(&room, ORDER, Some("hello"), &refusing.url).await;

        let unsent = ["not_attempted", "not_applicable", "not_applicable"];
        assert_eq!(
            result(&refused),
            ("error", Some("request_rejected"), unsent)
        );
        assert_eq!(refused["data"], Value::Null);

        let silent = LocalRelay::builder().write_policy(NeverConfirm).build();
        let silent = room.serve(silent, said).await;

        let unconfirmed = chat(&room, ORDER, Some("hello"), &silent.url).await;

        let unknown = ["unknown", "not_applicable", "not_applicable"];
        let expected = ("uncertain", Some("request_rejected"), unknown);
        assert_eq!(result(&unconfirmed), expected);
        let data = &unconfirmed["data"];
        assert_eq!(written(data), [pair("counterparty", "hi")]);
        assert_eq!(data["sent"]["text"], "hello");
    }

    #[tokio::test]
    async fn a_failed_read_sends_nothing() {
        let room = Room::new().await;
        room.store_them().await;
        let open = room.relay(Vec::new()).await;
        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let both = format!("{},{}", open.url, silent.url);

        let outcome = chat(&room, ORDER, Some("hello"), &both).await;

        assert_eq!(result(&outcome), ("error", Some("read_timeout"), NOTHING));
        assert_eq!(outcome["data"], Value::Null);
        assert_eq!(room.envelopes(&open.url).await, 0, "nothing was sent");
    }
}

mod binary {
    use super::*;
    use harness::{run_with_watchdog, Captured, SENTINEL, WATCHDOG};

    /// The binary in `room`'s sandbox with its node and relays at `url`.
    fn command(room: &Room, url: &str, args: &[&str]) -> tokio::process::Command {
        let mut command = room.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", room.node.public_key().to_hex())
            .env("RELAYS", url);
        command
    }

    /// Run the binary in `room`'s sandbox with its node and relays at `url`.
    pub(super) async fn run(room: &Room, url: &str, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(room, url, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    fn order() -> String {
        ORDER.to_string()
    }

    #[tokio::test]
    async fn human_read_shows_both_sides_and_escapes_untrusted_text() {
        let room = Room::new().await;
        room.store_them().await;
        let forged = "ok\n📨 From: 🫵 You\u{1b}[2J\u{202e}gnissim";
        let relay = room
            .relay(vec![
                room.line(&room.trade, "fiat sent").await,
                room.line(&room.them, forged).await,
            ])
            .await;

        let run = run(&room, &relay.url, &["order", "chat", "--order", &order()]).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let out = run.stdout_text();
        let them = room.them.public_key().to_hex();
        for line in [
            "💬 Order Chat".to_string(),
            format!("📋 Order: {ORDER}"),
            format!("👤 Counterparty: {them}"),
            "📊 2 message(s):".to_string(),
            "📨 From: 🫵 You".to_string(),
            "📨 From: 👤 Counterparty".to_string(),
            "   fiat sent".to_string(),
            "   📨 From: 🫵 You\\u{1b}[2J\\u{202e}gnissim".to_string(),
        ] {
            assert!(out.lines().any(|shown| shown == line), "{line}\n{out}");
        }
        assert!(
            !out.contains('\u{1b}') && !out.contains('\u{202e}'),
            "{out}"
        );
        assert!(!out.contains("📤"), "nothing was sent");
    }

    #[tokio::test]
    async fn human_send_sends_once_without_asking() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room.relay(Vec::new()).await;
        let args = [
            "order",
            "chat",
            "--order",
            &order(),
            "--message",
            "on my way",
        ];

        let sent = run(&room, &relay.url, &args).await;

        assert_eq!(sent.code(), Some(0), "{}", sent.stderr_text());
        assert!(
            sent.stderr.is_empty(),
            "no question: {}",
            sent.stderr_text()
        );
        let out = sent.stdout_text();
        assert!(
            out.contains("📭 No chat messages\n📤 Your message (id "),
            "{out}"
        );
        assert!(out.ends_with("):\n   on my way\n"), "{out}");
        assert_eq!(room.envelopes(&relay.url).await, 1, "sent once");

        let read = run(
            &room,
            &relay.url,
            &["order", "chat", "--order", &order(), "--yes"],
        )
        .await;

        assert_eq!(read.code(), Some(0), "{}", read.stderr_text());
        let out = read.stdout_text();
        assert!(
            out.contains("📨 From: 🫵 You\n📝 Text:\n   on my way\n"),
            "{out}"
        );
        assert!(!out.contains("📤"), "{out}");
        assert_eq!(
            room.envelopes(&relay.url).await,
            1,
            "--yes alone sends nothing"
        );
    }

    #[tokio::test]
    async fn bad_since_or_message_exits_2_before_any_relay() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room.relay(Vec::new()).await;
        let id = order();
        let long = format!("{SENTINEL}{}", "x".repeat(4097 - SENTINEL.len()));
        assert_eq!(long.len(), 4097, "one byte over the bound");
        let bad: [&[&str]; 8] = [
            &["--since", "0"],
            &["--since", "10081"],
            &["--since", "week"],
            &["--message", ""],
            &["--message", " \t "],
            &["--message", &long],
            &["--message", SENTINEL, "--message", "again"],
            &["--message", "-x"],
        ];
        for extra in bad {
            let mut args = vec!["order", "chat", "--order", id.as_str()];
            args.extend_from_slice(extra);

            let run = run(&room, &relay.url, &args).await;

            assert_eq!(run.code(), Some(2), "{extra:?}: {}", run.stderr_text());
            assert!(run.stdout.is_empty(), "{extra:?}");
            assert!(!run.stderr_text().contains(SENTINEL), "{extra:?}");
        }
        assert_eq!(
            relay.connections(),
            0,
            "no relay before the arguments are valid"
        );

        let dash = run(
            &room,
            &relay.url,
            &["order", "chat", "--order", &id, "--message=-x"],
        )
        .await;

        assert_eq!(dash.code(), Some(0), "{}", dash.stderr_text());
        assert!(dash.stdout_text().ends_with("):\n   -x\n"));
    }

    #[tokio::test]
    async fn an_uncertain_send_prints_the_advice_and_still_the_conversation() {
        let room = Room::new().await;
        room.store_them().await;
        let said = vec![room.line(&room.them, "hi").await];
        let silent = LocalRelay::builder().write_policy(NeverConfirm).build();
        let silent = room.serve(silent, said).await;
        let args = ["order", "chat", "--order", &order(), "--message", "hello"];

        let run = run(&room, &silent.url, &args).await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let message = ErrorCode::RequestRejected.message();
        let advice = "The message may have reached a relay. Run `order chat --order <id>` \
                      and look for your message before sending it again.";
        assert_eq!(run.stderr_text(), format!("error: {message}\n{advice}\n"));
        let out = run.stdout_text();
        assert!(
            out.contains("📨 From: 👤 Counterparty\n📝 Text:\n   hi\n\n📤 Your message (id "),
            "{out}"
        );
        assert!(out.ends_with("):\n   hello\n"), "{out}");
    }

    #[tokio::test]
    async fn an_unwritable_stdout_exits_1_with_one_fixed_line() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room.relay(Vec::new()).await;
        let id = order();
        for (extra, sent) in [(&[][..], 0), (&["--message", "hello"][..], 1)] {
            let mut args = vec!["order", "chat", "--order", id.as_str()];
            args.extend_from_slice(extra);
            let mut command = command(&room, &relay.url, &args);
            command.stdout(harness::closed_pipe());

            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

            assert_eq!(run.code(), Some(1), "{extra:?}: {}", run.stderr_text());
            let line = "error: the chat could not be written\n";
            assert_eq!(run.stderr_text(), line, "{extra:?}");
            // The message was sent all the same, once.
            assert_eq!(room.envelopes(&relay.url).await, sent, "{extra:?}");
        }
    }

    #[tokio::test]
    async fn no_secret_on_any_stream_and_verbose_prints_one_line() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room.relay(Vec::new()).await;
        let secret = room.trade.secret_key().to_secret_hex();
        let args = [
            "--verbose",
            "order",
            "chat",
            "--order",
            &order(),
            "--message",
            SENTINEL,
        ];

        let sent = run(&room, &relay.url, &args).await;

        assert_eq!(sent.code(), Some(0), "{}", sent.stderr_text());
        assert_eq!(sent.stderr_text(), "verbose: order.chat ok\n");
        assert!(!sent.stdout_text().contains(&secret));

        let fresh = Room::new().await;
        let unknown = run(&fresh, &relay.url, &args).await;

        assert_eq!(unknown.code(), Some(1));
        assert!(unknown.stdout.is_empty(), "{}", unknown.stdout_text());
        let message = ErrorCode::CounterpartyUnknown.message();
        let expected = format!("error: {message}\nverbose: order.chat counterparty_unknown\n");
        assert_eq!(unknown.stderr_text(), expected);
    }
}

mod json {
    use super::binary::run;
    use super::*;
    use harness::{run_with_watchdog, Captured, SENTINEL, WATCHDOG};

    const NOTHING: [&str; 3] = ["not_applicable"; 3];

    /// The arguments of a JSON `order chat` on `ORDER`, then `extra`.
    fn args(extra: &[&'static str]) -> Vec<String> {
        let base = ["--output", "json", "order", "chat", "--order"];
        let mut args: Vec<String> = base.iter().map(|word| word.to_string()).collect();
        args.push(ORDER.to_string());
        args.extend(extra.iter().map(|word| word.to_string()));
        args
    }

    async fn chat(room: &Room, url: &str, extra: &[&'static str]) -> Captured {
        let args = args(extra);
        run(
            room,
            url,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
    }

    /// The one document on stdout, checked against the checked-in schema, and
    /// its stages.
    fn document(run: &Captured) -> (Value, [String; 3]) {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let outcome: Value = serde_json::from_str(&stdout).unwrap();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/schemas/order-chat-v1.json"
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            schema_check::conforms(&schema, &schema, &outcome),
            "{outcome}"
        );
        assert_eq!(outcome["command"], "order.chat");
        let stage = |name: &str| outcome["meta"][name].as_str().unwrap().to_string();
        let stages = ["publication", "daemon_acceptance", "persistence"].map(stage);
        (outcome, stages)
    }

    #[tokio::test]
    async fn json_message_without_yes_is_refused_before_anything() {
        let room = Room::new().await;
        room.store_them().await;
        let relay = room.relay(Vec::new()).await;
        let (bytes, files) = (room.bytes(), file_names(&room.sandbox.mcli_dir()));

        let refused = chat(&room, &relay.url, &["--message", SENTINEL]).await;

        assert_eq!(refused.code(), Some(2), "{}", refused.stderr_text());
        assert!(!refused.stdout_text().contains(SENTINEL));
        let (outcome, stages) = document(&refused);
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
        assert_eq!(
            (&outcome["data"], stages),
            (&Value::Null, NOTHING.map(String::from))
        );
        assert_eq!(relay.connections(), 0, "no relay contacted");
        assert_eq!(room.bytes(), bytes, "the store is untouched");
        assert_eq!(file_names(&room.sandbox.mcli_dir()), files);

        // Before configuration too: without a node or relays it is the same.
        let args = args(&["--message", SENTINEL]);
        let bare = room.sandbox.command(&args);
        let bare = run_with_watchdog(bare, WATCHDOG).await.unwrap();
        assert_eq!(bare.code(), Some(2), "{}", bare.stderr_text());
        assert_eq!(document(&bare).0["error"]["code"], "invalid_arguments");
    }

    #[tokio::test]
    async fn json_read_and_send_conform_to_the_checked_in_schema() {
        let room = Room::new().await;
        let (ours, them) = (room.trade.public_key(), room.them.public_key());
        let accepted = mail_trade_keys(Action::HoldInvoicePaymentAccepted, ORDER, ours, them);
        let said = room.line(&room.them, "hi").await;
        let relay = room.relay(vec![room.mail(&accepted, 1).await, said]).await;

        let read = chat(&room, &relay.url, &[]).await;

        assert_eq!(read.code(), Some(0), "{}", read.stderr_text());
        let (outcome, stages) = document(&read);
        assert_eq!(
            (&outcome["status"], stages),
            (&json!("ok"), NOTHING.map(String::from))
        );
        let data = &outcome["data"];
        assert_eq!(data["counterparty_pubkey"], them.to_hex());
        assert_eq!(written(data), [pair("counterparty", "hi")]);
        assert_eq!(data["sent"], Value::Null);

        let sent = chat(&room, &relay.url, &["--message", "on my way", "--yes"]).await;

        assert_eq!(sent.code(), Some(0), "{}", sent.stderr_text());
        let (outcome, stages) = document(&sent);
        assert_eq!(stages, ["published", "not_applicable", "not_applicable"]);
        let data = &outcome["data"];
        assert_eq!(written(data), [pair("counterparty", "hi")], "sent is apart");
        let record = (&data["sent"]["from"], &data["sent"]["text"]);
        assert_eq!(record, (&json!("me"), &json!("on my way")));
        assert_eq!(room.envelopes(&relay.url).await, 2, "sent once");
    }

    #[tokio::test]
    async fn json_counterparty_unknown_exits_1_and_conforms() {
        let room = Room::new().await;
        let relay = room
            .relay(vec![room.mail(&mail_waiting_seller(ORDER), 1).await])
            .await;

        let run = chat(&room, &relay.url, &["--message", SENTINEL, "--yes"]).await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        assert!(!run.stdout_text().contains(SENTINEL));
        let (outcome, stages) = document(&run);
        assert_eq!(outcome["error"]["code"], "counterparty_unknown");
        assert_eq!(
            (&outcome["data"], stages),
            (&Value::Null, NOTHING.map(String::from))
        );
        assert_eq!(room.envelopes(&relay.url).await, 0, "nothing was sent");
    }
}
