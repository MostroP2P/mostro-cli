//! Tests for `order chat` (spec 019), unit 1: the counterparty column of the
//! store and the lookup of the counterparty in the node's messages.
//!
//! Every store is an isolated sandbox seeded the way the CLI creates one; the
//! node's mail sits on an in-process loopback relay, authored by generated node
//! keys. Nothing else is contacted.

use std::path::Path;
use std::time::Duration;

use mostro_core::prelude::{Action, Message};
use nostr_sdk::prelude::{Event, Keys, LocalRelay, PublicKey, Timestamp};
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
