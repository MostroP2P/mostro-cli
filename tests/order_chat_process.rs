//! Tests for `order chat` (spec 019): the counterparty column of the store and
//! the lookup of the counterparty in the node's messages (unit 1), and the
//! chat read and send (unit 2).
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
use mostro_core::prelude::{derive_chat_keys, wrap_chat_message, Action, Message};
use nostr_sdk::prelude::{
    Event, Filter, Keys, Kind, LocalRelay, LocalRelayBuilderNip42, MachineReadablePrefix,
    PublicKey, Timestamp, WritePolicy, WritePolicyResult,
};
use serde_json::Value;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

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
