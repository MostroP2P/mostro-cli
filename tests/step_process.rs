//! Tests for `order fiat-sent`, `order release` and `order cancel` (spec 007).
//!
//! Slice 1 holds the self-tests of the fixture built for the commands: the
//! scripted daemon that applies the source's `fiat_sent_action`,
//! `release_action` and `cancel_action_generic` rules. The commands' own
//! scenarios are added with the slices that implement them.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::Status;
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, MessageKind, Payload,
    Transport, WrapOptions,
};
use nostr_sdk::prelude::{Client, ClientNotification, Filter, Keys, Kind, LocalRelay, PublicKey};
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{info_event, seed_identity, step_daemon, FakeMostro, Respond, Traded};
use harness::{Backend, LoopbackRelay, Sandbox};
use mostro_client::store::{Role, Store};

const REQUEST_ID: u64 = 7070;
const WAIT: Duration = Duration::from_millis(700);

/// Rejects every write with a machine-readable refusal.
#[derive(Debug)]
struct BlockWrites;

impl nostr_sdk::prelude::WritePolicy for BlockWrites {
    fn admit_event<'a>(
        &'a self,
        _event: &'a nostr_sdk::prelude::Event,
        _addr: &'a std::net::SocketAddr,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = nostr_sdk::prelude::WritePolicyResult> + Send + 'a>,
    > {
        Box::pin(async {
            nostr_sdk::prelude::WritePolicyResult::reject(
                nostr_sdk::prelude::MachineReadablePrefix::Blocked,
                "writes are off",
            )
        })
    }
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// A client listening on one key and able to send as any identity and trade key.
struct Peer {
    client: Client,
    node: PublicKey,
}

impl Peer {
    async fn connect(relay: &LoopbackRelay, node: PublicKey) -> Self {
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        Self { client, node }
    }

    /// Listen on every key in `keys`, run `act`, and return what the node sent
    /// to each of them meanwhile, in the order of `keys`.
    async fn hear_all(
        &self,
        keys: &[&Keys],
        act: impl std::future::Future<Output = ()>,
    ) -> Vec<Vec<Message>> {
        let mut notifications = self.client.notifications();
        for key in keys {
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(key.public_key())
                .author(self.node)
                .limit(0);
            self.client.subscribe(filter).await.unwrap();
        }
        act.await;
        let mut heard = vec![Vec::new(); keys.len()];
        let _ = timeout(WAIT, async {
            while let Some(notification) = notifications.next().await {
                let ClientNotification::Event { event, .. } = notification else {
                    continue;
                };
                for (index, key) in keys.iter().enumerate() {
                    if let Ok(Some(seen)) = unwrap_incoming(&event, key).await {
                        heard[index].push(seen.message);
                    }
                }
            }
        })
        .await;
        heard
    }

    async fn send(&self, message: &Message, identity: &Keys, trade: &Keys) {
        let event = wrap_message_with(
            Transport::Nip44Direct,
            message,
            identity,
            trade,
            self.node,
            WrapOptions::default(),
        )
        .await
        .unwrap();
        self.client.send_event(&event).await.unwrap();
    }
}

fn step(action: Action, order: Uuid, payload: Option<Payload>) -> Message {
    Message::new_order(Some(order), Some(REQUEST_ID), None, action, payload)
}

struct World {
    _relay: LoopbackRelay,
    fake: FakeMostro,
    peer: Peer,
    trades: Arc<Mutex<Vec<Traded>>>,
    seller: Keys,
    buyer: Keys,
}

async fn world(trades: impl FnOnce(PublicKey, PublicKey) -> Vec<Traded>) -> World {
    let (seller, buyer) = (Keys::generate(), Keys::generate());
    let (respond, trades): (Respond, _) =
        step_daemon(trades(seller.public_key(), buyer.public_key()));
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
    let peer = Peer::connect(&relay, fake.pubkey()).await;
    World {
        _relay: relay,
        fake,
        peer,
        trades,
        seller,
        buyer,
    }
}

impl World {
    /// Send `message` as `from`; what `from` and `other` hear meanwhile.
    async fn act(
        &self,
        from: &Keys,
        other: &Keys,
        message: Message,
    ) -> (Vec<Message>, Vec<Message>) {
        let identity = Keys::generate();
        let mut heard = self
            .peer
            .hear_all(&[from, other], self.peer.send(&message, &identity, from))
            .await;
        let other = heard.pop().unwrap();
        (heard.pop().unwrap(), other)
    }

    fn status(&self) -> Status {
        self.trades.lock().unwrap()[0].status
    }
}

fn only(replies: Vec<Message>) -> MessageKind {
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies[0].get_inner_message_kind().clone()
}

fn actions(replies: &[Message]) -> Vec<Action> {
    replies
        .iter()
        .map(|reply| reply.get_inner_message_kind().action.clone())
        .collect()
}

fn reason(kind: &MessageKind) -> CantDoReason {
    assert_eq!(kind.action, Action::CantDo);
    match &kind.payload {
        Some(Payload::CantDo(Some(reason))) => reason.clone(),
        other => panic!("a refusal, got {other:?}"),
    }
}

fn active(seller: PublicKey, buyer: PublicKey) -> Vec<Traded> {
    vec![Traded::active(id(1), seller, buyer)]
}

fn at(status: Status) -> impl FnOnce(PublicKey, PublicKey) -> Vec<Traded> {
    move |seller, buyer| vec![Traded::active(id(1), seller, buyer).with_status(status)]
}

#[tokio::test]
async fn fiat_sent_by_the_buyer_moves_the_order_and_notifies_both_sides() {
    let world = world(active).await;

    let (buyer, seller) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::FiatSent, id(1), None),
        )
        .await;

    let reply = only(buyer);
    assert_eq!(reply.action, Action::FiatSentOk);
    assert_eq!(reply.request_id, Some(REQUEST_ID), "echoed to the sender");
    assert_eq!(reply.id, Some(id(1)));
    let notice = only(seller);
    assert_eq!(notice.action, Action::FiatSentOk);
    assert_eq!(notice.request_id, None, "the notice has no request id");
    assert_eq!(world.status(), Status::FiatSent);
    let seen = world.fake.received();
    assert_eq!((seen.len(), seen[0].trade_index), (1, None));
}

#[tokio::test]
async fn fiat_sent_keeps_the_next_trade_key_of_a_range_order() {
    let world = world(active).await;
    let next = Payload::NextTrade("02".repeat(32), 9);

    world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::FiatSent, id(1), Some(next)),
        )
        .await;

    assert_eq!(
        world.trades.lock().unwrap()[0].next_trade,
        Some(("02".repeat(32), 9))
    );
    assert_eq!(
        world.fake.received()[0].next_trade,
        Some(("02".repeat(32), 9))
    );
}

#[tokio::test]
async fn fiat_sent_is_refused_to_the_seller_and_outside_the_active_status() {
    let world = world(active).await;
    let (seller, _) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::FiatSent, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(seller)), CantDoReason::InvalidPubkey);
    assert_eq!(world.status(), Status::Active);

    let world = self::world(at(Status::FiatSent)).await;
    let (buyer, _) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::FiatSent, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(buyer)), CantDoReason::NotAllowedByStatus);
}

#[tokio::test]
async fn release_by_the_seller_settles_and_notifies_both_sides() {
    let world = world(at(Status::FiatSent)).await;

    let (seller, buyer) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Release, id(1), None),
        )
        .await;

    assert_eq!(
        actions(&seller),
        [Action::HoldInvoicePaymentSettled, Action::Rate]
    );
    let settled = seller[0].get_inner_message_kind();
    assert_eq!(settled.request_id, Some(REQUEST_ID));
    assert_eq!(seller[1].get_inner_message_kind().request_id, None);
    assert_eq!(actions(&buyer), [Action::Released]);
    assert_eq!(world.status(), Status::SettledHoldInvoice);
}

#[tokio::test]
async fn release_is_refused_to_the_buyer_and_before_fiat_was_sent() {
    let world = world(at(Status::FiatSent)).await;
    let (buyer, _) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::Release, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(buyer)), CantDoReason::InvalidPeer);

    let world = self::world(active).await;
    let (seller, _) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Release, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(seller)), CantDoReason::NotAllowedByStatus);
    assert_eq!(world.status(), Status::Active);
}

#[tokio::test]
async fn release_is_accepted_during_a_dispute() {
    let world = world(at(Status::Dispute)).await;

    let (seller, _) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Release, id(1), None),
        )
        .await;

    assert_eq!(actions(&seller)[0], Action::HoldInvoicePaymentSettled);
}

#[tokio::test]
async fn a_pending_order_is_canceled_by_its_maker_only() {
    let world = world(|seller, _| vec![Traded::pending(id(1), seller)]).await;
    let stranger = Keys::generate();
    let (heard, _) = world
        .act(&stranger, &world.buyer, step(Action::Cancel, id(1), None))
        .await;
    assert_eq!(reason(&only(heard)), CantDoReason::IsNotYourOrder);

    let (maker, _) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Cancel, id(1), None),
        )
        .await;

    let reply = only(maker);
    assert_eq!(reply.action, Action::Canceled);
    assert_eq!(reply.request_id, Some(REQUEST_ID));
    assert_eq!(world.status(), Status::Canceled);
}

#[tokio::test]
async fn an_order_waiting_for_payment_is_canceled_and_the_other_side_is_told() {
    let world = world(at(Status::WaitingPayment)).await;

    let (buyer, seller) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::Cancel, id(1), None),
        )
        .await;

    assert_eq!(actions(&buyer), [Action::Canceled]);
    assert_eq!(actions(&seller), [Action::Canceled]);
    assert_eq!(world.status(), Status::Canceled);
}

#[tokio::test]
async fn a_cooperative_cancel_takes_two_parties_and_the_initiator_cannot_confirm() {
    let world = world(active).await;

    let (buyer, seller) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::Cancel, id(1), None),
        )
        .await;
    assert_eq!(actions(&buyer), [Action::CooperativeCancelInitiatedByYou]);
    assert_eq!(only(buyer).request_id, Some(REQUEST_ID));
    assert_eq!(actions(&seller), [Action::CooperativeCancelInitiatedByPeer]);
    assert_eq!(world.status(), Status::Active);

    let (again, _) = world
        .act(
            &world.buyer,
            &world.seller,
            step(Action::Cancel, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(again)), CantDoReason::InvalidPubkey);

    let (seller, buyer) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Cancel, id(1), None),
        )
        .await;
    assert_eq!(actions(&seller), [Action::CooperativeCancelAccepted]);
    assert_eq!(actions(&buyer), [Action::CooperativeCancelAccepted]);
    assert_eq!(world.status(), Status::CooperativelyCanceled);
}

#[tokio::test]
async fn a_canceled_order_cannot_be_canceled_again_and_a_stranger_is_refused() {
    let world = world(at(Status::Canceled)).await;
    let (seller, _) = world
        .act(
            &world.seller,
            &world.buyer,
            step(Action::Cancel, id(1), None),
        )
        .await;
    assert_eq!(reason(&only(seller)), CantDoReason::OrderAlreadyCanceled);

    let world = self::world(active).await;
    let stranger = Keys::generate();
    let (heard, _) = world
        .act(&stranger, &world.buyer, step(Action::Cancel, id(1), None))
        .await;
    assert_eq!(reason(&only(heard)), CantDoReason::InvalidPubkey);
}

#[tokio::test]
async fn an_unknown_order_is_not_found_for_all_three_steps() {
    let world = world(active).await;
    for action in [Action::FiatSent, Action::Release, Action::Cancel] {
        let (heard, _) = world
            .act(&world.buyer, &world.seller, step(action, id(99), None))
            .await;
        assert_eq!(reason(&only(heard)), CantDoReason::NotFound);
    }
}

// --- The store (spec 007, slice 2) ------------------------------------------

mod store {
    use super::*;

    async fn stored(update: &'static str) -> (Sandbox, Store) {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = store.next_trade().await.unwrap();
        let order = Traded::active(id(1), trade.keys.public_key(), trade.keys.public_key()).small();
        store.save_order(&order, &trade, REQUEST_ID).await.unwrap();
        let options =
            sqlx::sqlite::SqliteConnectOptions::new().filename(sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(update).execute(&pool).await.unwrap();
        pool.close().await;
        (sandbox, store)
    }

    #[tokio::test]
    async fn a_range_order_of_mine_with_room_left_has_room() {
        let (_sandbox, store) = stored(
            "UPDATE orders SET is_mine = 1, min_amount = 10, max_amount = 100, fiat_amount = 40",
        )
        .await;

        assert!(store.range_has_room(id(1)).await.unwrap());
    }

    #[tokio::test]
    async fn an_exhausted_range_a_fixed_order_and_an_unknown_order_have_none() {
        let (_sandbox, store) = stored(
            "UPDATE orders SET is_mine = 1, min_amount = 10, max_amount = 100, fiat_amount = 95",
        )
        .await;
        assert!(!store.range_has_room(id(1)).await.unwrap());
        assert!(!store.range_has_room(id(2)).await.unwrap(), "unknown");

        let (_sandbox, store) = stored(
            "UPDATE orders SET is_mine = 1, min_amount = NULL, max_amount = NULL, fiat_amount = 40",
        )
        .await;
        assert!(!store.range_has_room(id(1)).await.unwrap(), "fixed");
    }

    async fn marker(sandbox: &Sandbox, order: Uuid) -> Option<bool> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar("SELECT is_mine FROM orders WHERE id = ?1")
            .bind(order.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    fn range(order: Uuid, owner: PublicKey) -> mostro_core::order::SmallOrder {
        let mut small = Traded::active(order, owner, owner).small();
        small.min_amount = Some(10);
        small.max_amount = Some(100);
        small.fiat_amount = 40;
        small
    }

    #[tokio::test]
    async fn the_maker_marker_is_exact_and_only_the_makers_range_has_room() {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = store.next_trade().await.unwrap();
        let mine = range(id(1), trade.keys.public_key());
        let taken = range(id(2), trade.keys.public_key());

        store
            .save_order_as(&mine, &trade, REQUEST_ID, Role::Maker)
            .await
            .unwrap();
        store
            .save_order_as(&taken, &trade, REQUEST_ID, Role::Taker)
            .await
            .unwrap();

        assert_eq!(marker(&sandbox, id(1)).await, Some(true));
        assert_eq!(marker(&sandbox, id(2)).await, Some(false));
        assert!(store.range_has_room(id(1)).await.unwrap(), "the maker's");
        assert!(
            !store.range_has_room(id(2)).await.unwrap(),
            "a taker needs no next key"
        );
    }

    #[tokio::test]
    async fn saving_again_keeps_the_role_of_the_latest_save_and_save_order_is_the_makers() {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = store.next_trade().await.unwrap();
        let order = range(id(1), trade.keys.public_key());

        store.save_order(&order, &trade, REQUEST_ID).await.unwrap();
        assert_eq!(marker(&sandbox, id(1)).await, Some(true), "the default");
        store
            .save_order_as(&order, &trade, REQUEST_ID, Role::Taker)
            .await
            .unwrap();
        assert_eq!(marker(&sandbox, id(1)).await, Some(false), "updated");
    }

    #[tokio::test]
    async fn reading_the_range_changes_no_byte_and_a_missing_store_is_not_created() {
        let (sandbox, store) = stored(
            "UPDATE orders SET is_mine = 1, min_amount = 10, max_amount = 100, fiat_amount = 40",
        )
        .await;
        let db = sandbox.mcli_dir().join("mcli.db");
        let before = std::fs::read(&db).unwrap();

        store.range_has_room(id(1)).await.unwrap();

        assert_eq!(std::fs::read(&db).unwrap(), before);
        let empty = Sandbox::new().unwrap();
        let missing = Store::in_dir(&empty.mcli_dir());
        assert!(missing.range_has_room(id(1)).await.is_err());
        assert!(!empty.mcli_dir().join("mcli.db").exists());
    }
}

// --- The operation (spec 007, slice 3) --------------------------------------

mod operation {
    use super::*;
    use fake_mostro::{Outgoing, Received};
    use mostro_client::commands::order::step::{order_step, StepOutcome};
    use mostro_client::commands::order::WriteOptions;
    use mostro_client::msg::{ExchangeLimits, PublicReadConfig};
    use mostro_client::outcome::step::{StepInput, StepKind, StepNext, StepResult};
    use mostro_client::outcome::{ErrorCode, Status as Outcome};
    use mostro_client::store::Trade;
    use serde_json::{json, Value};

    struct Holder {
        _relay: LoopbackRelay,
        fake: FakeMostro,
        config: PublicReadConfig,
        identity: Keys,
        sandbox: Sandbox,
        store: Store,
        me: Keys,
        other: Keys,
    }

    /// A user holding orders 1 and 2 under one trade key `me`, against a node
    /// that answers with the step daemon over `trades(me, other)`.
    async fn holder(
        trades: impl FnOnce(PublicKey, PublicKey) -> Vec<Traded>,
        pow: u8,
        wrap: impl FnOnce(Respond) -> Respond,
    ) -> Holder {
        holder_with(trades, (2, pow, false), wrap).await
    }

    /// A node advertising `(protocol, pow)`; its relay refuses every write when
    /// the last element is set.
    async fn holder_with(
        trades: impl FnOnce(PublicKey, PublicKey) -> Vec<Traded>,
        (protocol, pow, block_writes): (u8, u8, bool),
        wrap: impl FnOnce(Respond) -> Respond,
    ) -> Holder {
        let (me, other) = (Keys::generate(), Keys::generate());
        let trades = trades(me.public_key(), other.public_key());
        let (respond, _) = step_daemon(trades.clone());
        let node = Keys::generate();
        let relay = if block_writes {
            LocalRelay::builder()
                .write_policy(super::BlockWrites)
                .build()
        } else {
            LocalRelay::new()
        };
        relay
            .add_event(info_event(&node, Some(protocol), Some(pow), None, Some(10)))
            .await
            .unwrap();
        let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap();
        let fake = FakeMostro::start(&relay.url, node, wrap(respond)).await;
        let config = PublicReadConfig::new(fake.pubkey(), &relay.url).unwrap();
        let sandbox = Sandbox::new().unwrap();
        let identity = seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = Trade {
            index: 1,
            keys: me.clone(),
        };
        for order in [id(1), id(2)] {
            let mut small = Traded::active(order, me.public_key(), other.public_key()).small();
            small.status = Some(trades.first().map_or(Status::Active, |trade| trade.status));
            store.save_order(&small, &trade, REQUEST_ID).await.unwrap();
        }
        Holder {
            _relay: relay,
            fake,
            config,
            identity,
            sandbox,
            store,
            me,
            other,
        }
    }

    /// `me` is the buyer of an active order.
    fn buying(me: PublicKey, other: PublicKey) -> Vec<Traded> {
        vec![
            Traded::active(id(1), other, me),
            Traded::active(id(2), other, me),
        ]
    }

    fn selling_at(status: Status) -> impl FnOnce(PublicKey, PublicKey) -> Vec<Traded> {
        move |me, other| {
            vec![
                Traded::active(id(1), me, other).with_status(status),
                Traded::active(id(2), me, other).with_status(status),
            ]
        }
    }

    fn limits() -> ExchangeLimits {
        ExchangeLimits {
            connect: Duration::from_secs(2),
            probe: Duration::from_millis(400),
            reply: Duration::from_millis(1_500),
            cleanup: Duration::from_millis(500),
            ..ExchangeLimits::default()
        }
    }

    const OPTIONS: WriteOptions = WriteOptions {
        pow: 0,
        check_protocol: true,
    };

    async fn take(holder: &Holder, kind: StepKind, order: Uuid) -> StepOutcome {
        order_step(
            &StepInput::new(kind, order),
            &holder.config,
            &holder.identity,
            &holder.store,
            OPTIONS,
            &limits(),
        )
        .await
    }

    fn database(holder: &Holder) -> Vec<u8> {
        std::fs::read(holder.sandbox.mcli_dir().join("mcli.db")).unwrap()
    }

    fn stages(outcome: &StepOutcome) -> Value {
        let mut meta = serde_json::to_value(outcome).unwrap()["meta"].clone();
        meta.as_object_mut().unwrap().remove("snapshot");
        meta
    }

    fn meta(publication: &str, acceptance: &str, persistence: &str) -> Value {
        json!({"publication": publication, "daemon_acceptance": acceptance,
               "persistence": persistence})
    }

    async fn scalar<T>(holder: &Holder, sql: &'static str, order: Option<Uuid>) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let mut query = sqlx::query_scalar(sql);
        if let Some(order) = order {
            query = query.bind(order.to_string());
        }
        let value = query.fetch_one(&pool).await.unwrap();
        pool.close().await;
        value
    }

    async fn status_of(holder: &Holder, order: Uuid) -> String {
        scalar(
            holder,
            "SELECT status FROM orders WHERE id = ?1",
            Some(order),
        )
        .await
    }

    async fn last_index(holder: &Holder) -> i64 {
        scalar(holder, "SELECT last_trade_index FROM users", None).await
    }

    async fn sql(holder: &Holder, statement: &'static str) {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(statement).execute(&pool).await.unwrap();
        pool.close().await;
    }

    fn warnings(outcome: &StepOutcome) -> Vec<String> {
        serde_json::to_value(outcome).unwrap()["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|warning| warning["code"].as_str().unwrap().to_string())
            .collect()
    }

    fn code(outcome: &StepOutcome) -> Option<ErrorCode> {
        outcome.error_code()
    }

    fn with_decoys(decoys: Vec<Outgoing>) -> impl FnOnce(Respond) -> Respond {
        move |inner| {
            Arc::new(move |request: &Received| {
                let mut out = decoys.clone();
                out.extend(inner(request));
                out
            })
        }
    }

    fn reply_with(action: Action, order: Option<Uuid>, request_id: Option<u64>) -> Message {
        Message::new_order(order, request_id, None, action, None)
    }

    /// The one request the node saw: signed by `me`, naming the order, with no
    /// payload, no trade index and no next trade key.
    fn assert_one_plain_request(holder: &Holder, action: Action, order: Uuid) {
        let seen = holder.fake.received();
        assert_eq!(seen.len(), 1, "one publication, never retried");
        assert_eq!(seen[0].action, action);
        assert_eq!(seen[0].order_id, Some(order));
        assert_eq!(seen[0].author, holder.me.public_key());
        assert_eq!(seen[0].identity, holder.identity.public_key());
        assert!(seen[0].signature_valid);
        assert!(seen[0].request_id.is_some());
        assert_eq!(seen[0].trade_index, None);
        assert_eq!(seen[0].next_trade, None, "range orders are spec 008");
        assert!(seen[0].order.is_none() && seen[0].invoice.is_none());
    }

    #[tokio::test]
    async fn fiat_sent_is_accepted_and_the_local_status_follows() {
        let holder = holder(buying, 0, |respond| respond).await;
        let index = last_index(&holder).await;

        let outcome = take(&holder, StepKind::FiatSent, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), StepResult::FiatSent);
        assert_eq!(data.next_step(), Some(StepNext::WaitForRelease));
        assert_eq!(data.order_id(), id(1).to_string());
        assert_eq!(data.trade_pubkey(), holder.me.public_key().to_hex());
        assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
        assert!(warnings(&outcome).is_empty());
        assert_eq!(status_of(&holder, id(1)).await, "fiat-sent");
        assert_eq!(status_of(&holder, id(2)).await, "active", "only that order");
        assert_eq!(last_index(&holder).await, index, "no index is spent");
        assert_one_plain_request(&holder, Action::FiatSent, id(1));
    }

    #[tokio::test]
    async fn release_is_accepted_and_the_order_is_settled_locally() {
        let holder = holder(selling_at(Status::FiatSent), 0, |respond| respond).await;

        let outcome = take(&holder, StepKind::Release, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), StepResult::Released);
        assert_eq!(data.next_step(), None);
        assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
        assert_eq!(status_of(&holder, id(1)).await, "settled-hold-invoice");
        assert_one_plain_request(&holder, Action::Release, id(1));
    }

    #[tokio::test]
    async fn a_pending_order_is_canceled_by_its_maker() {
        let holder = holder(
            |me, _| vec![Traded::pending(id(1), me), Traded::pending(id(2), me)],
            0,
            |respond| respond,
        )
        .await;

        let outcome = take(&holder, StepKind::Cancel, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(outcome.data().expect("data").result(), StepResult::Canceled);
        assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
        assert_eq!(status_of(&holder, id(1)).await, "canceled");
        assert_one_plain_request(&holder, Action::Cancel, id(1));
    }

    #[tokio::test]
    async fn the_first_step_of_a_cooperative_cancel_is_a_request_and_changes_nothing_locally() {
        let holder = holder(buying, 0, |respond| respond).await;
        let before = database(&holder);

        let outcome = take(&holder, StepKind::Cancel, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), StepResult::CancelRequested);
        assert_eq!(data.next_step(), Some(StepNext::WaitForPeerCancel));
        assert_eq!(
            stages(&outcome),
            meta("published", "accepted", "not_attempted")
        );
        assert_eq!(database(&holder), before, "no byte changed");
    }

    #[tokio::test]
    async fn confirming_the_peers_cooperative_cancel_cancels_the_order() {
        let holder = holder(
            |me, other| {
                let mut asked = Traded::active(id(1), other, me);
                asked.initiator = Some(other);
                vec![asked]
            },
            0,
            |respond| respond,
        )
        .await;

        let outcome = take(&holder, StepKind::Cancel, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), StepResult::CancelAccepted);
        assert_eq!(data.next_step(), None);
        assert_eq!(status_of(&holder, id(1)).await, "cooperatively-canceled");
    }

    #[tokio::test]
    async fn every_refusal_is_an_error_with_the_nodes_reason_and_changes_nothing() {
        type Trades = Box<dyn Fn(PublicKey, PublicKey) -> Vec<Traded>>;
        let cases: Vec<(&str, StepKind, Trades, &str)> = vec![
            (
                "fiat sent by the seller",
                StepKind::FiatSent,
                Box::new(|me, other| vec![Traded::active(id(1), me, other)]),
                "invalid_pubkey",
            ),
            (
                "fiat sent twice",
                StepKind::FiatSent,
                Box::new(|me, other| {
                    vec![Traded::active(id(1), other, me).with_status(Status::FiatSent)]
                }),
                "not_allowed_by_status",
            ),
            (
                "release by the buyer",
                StepKind::Release,
                Box::new(|me, other| {
                    vec![Traded::active(id(1), other, me).with_status(Status::FiatSent)]
                }),
                "invalid_peer",
            ),
            (
                "release before fiat was sent",
                StepKind::Release,
                Box::new(|me, other| vec![Traded::active(id(1), me, other)]),
                "not_allowed_by_status",
            ),
            (
                "cancel of a canceled order",
                StepKind::Cancel,
                Box::new(|me, other| {
                    vec![Traded::active(id(1), other, me).with_status(Status::Canceled)]
                }),
                "order_already_canceled",
            ),
            (
                "an order the node does not know",
                StepKind::Release,
                Box::new(|_, _| Vec::new()),
                "not_found",
            ),
        ];
        for (label, kind, trades, reason) in cases {
            let holder = holder(trades, 0, |respond| respond).await;
            let before = database(&holder);

            let outcome = take(&holder, kind, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Error, "{label}");
            assert_eq!(code(&outcome), Some(ErrorCode::NodeRefused), "{label}");
            let document = serde_json::to_value(&outcome).unwrap();
            assert_eq!(document["error"]["reason"], reason, "{label}");
            assert_eq!(
                stages(&outcome),
                meta("published", "rejected", "not_attempted"),
                "{label}"
            );
            assert_eq!(database(&holder), before, "{label}");
            assert_eq!(holder.fake.received().len(), 1, "{label}: never retried");
        }
    }

    #[tokio::test]
    async fn silence_is_uncertain_for_every_step_and_changes_nothing() {
        for kind in [StepKind::FiatSent, StepKind::Release, StepKind::Cancel] {
            let holder = holder(buying, 0, |_| fake_mostro::silent()).await;
            let before = database(&holder);

            let outcome = take(&holder, kind, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Uncertain, "{kind:?}");
            assert_eq!(code(&outcome), Some(ErrorCode::ReplyTimeout), "{kind:?}");
            assert!(outcome.data().is_none());
            assert_eq!(
                stages(&outcome),
                meta("published", "unknown", "not_attempted")
            );
            assert_eq!(holder.fake.received().len(), 1, "never retried");
            assert_eq!(database(&holder), before);
        }
    }

    #[tokio::test]
    async fn replies_that_do_not_fit_are_uncertain_and_change_nothing() {
        let answer = |action: Action, order: Uuid, payload: Option<Payload>| -> Respond {
            Arc::new(move |request: &Received| {
                vec![Outgoing::to_requester(Message::new_order(
                    Some(order),
                    request.request_id,
                    None,
                    action.clone(),
                    payload.clone(),
                ))]
            })
        };
        let cases: Vec<(&str, StepKind, Respond, ErrorCode)> = vec![
            (
                "another step's reply",
                StepKind::FiatSent,
                answer(Action::Canceled, id(1), None),
                ErrorCode::ReplyTimeout,
            ),
            (
                "a reply naming another order",
                StepKind::FiatSent,
                answer(Action::FiatSentOk, id(77), None),
                ErrorCode::ReplyMalformed,
            ),
            (
                "a release with a payload",
                StepKind::Release,
                answer(
                    Action::HoldInvoicePaymentSettled,
                    id(1),
                    Some(Payload::TextMessage("unexpected".to_string())),
                ),
                ErrorCode::ReplyMalformed,
            ),
            (
                "a cancel that carries an amount",
                StepKind::Cancel,
                answer(Action::Canceled, id(1), Some(Payload::Amount(5))),
                ErrorCode::ReplyMalformed,
            ),
            (
                "a cancel confirmation for a request",
                StepKind::Cancel,
                answer(Action::CooperativeCancelInitiatedByPeer, id(1), None),
                ErrorCode::ReplyTimeout,
            ),
        ];
        // An action the step does not allow never matches, so the wait times
        // out; an allowed action with a wrong id or payload is unusable.
        for (label, kind, respond, expected) in cases {
            let holder = holder(buying, 0, move |_| respond).await;
            let before = database(&holder);

            let outcome = take(&holder, kind, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Uncertain, "{label}");
            assert_eq!(code(&outcome), Some(expected), "{label}");
            assert_eq!(database(&holder), before, "{label}");
        }
    }

    #[tokio::test]
    async fn decoys_are_ignored_and_the_real_answer_is_used() {
        let decoys = vec![
            // A stale request id.
            Outgoing::to_requester(reply_with(Action::FiatSentOk, Some(id(1)), Some(1))),
            // Another author.
            Outgoing::to_requester(reply_with(
                Action::FiatSentOk,
                Some(id(1)),
                Some(REQUEST_ID),
            ))
            .signed_by(Keys::generate()),
            // Another addressee.
            Outgoing::to_requester(reply_with(
                Action::FiatSentOk,
                Some(id(1)),
                Some(REQUEST_ID),
            ))
            .addressed_to(Keys::generate().public_key()),
            // No request id: the node always echoes it to the sender.
            Outgoing::to_requester(reply_with(Action::Canceled, Some(id(1)), None)),
        ];
        let holder = holder(buying, 0, with_decoys(decoys)).await;

        let outcome = take(&holder, StepKind::FiatSent, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(outcome.data().expect("data").result(), StepResult::FiatSent);
        assert_eq!(status_of(&holder, id(1)).await, "fiat-sent");
    }

    #[tokio::test]
    async fn a_reply_without_the_request_id_is_never_taken_for_the_answer() {
        // The node echoes the request id to the sender, so a message that has
        // none (the notice meant for the other party) is not the answer.
        let decoys = vec![Outgoing::to_requester(reply_with(
            Action::CooperativeCancelAccepted,
            Some(id(1)),
            None,
        ))];
        let holder = holder(buying, 0, with_decoys(decoys)).await;

        let outcome = take(&holder, StepKind::Cancel, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(
            outcome.data().expect("data").result(),
            StepResult::CancelRequested,
            "the real answer, not the decoy"
        );
        assert_eq!(status_of(&holder, id(1)).await, "active");
    }

    #[tokio::test]
    async fn a_failed_status_update_still_reports_the_accepted_step_with_a_warning() {
        let holder = holder(selling_at(Status::FiatSent), 0, |respond| respond).await;
        sql(
            &holder,
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .await;

        let outcome = take(&holder, StepKind::Release, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(outcome.data().expect("data").result(), StepResult::Released);
        assert_eq!(stages(&outcome), meta("published", "accepted", "failed"));
        assert_eq!(warnings(&outcome), ["local_status_stale"]);
        assert_eq!(status_of(&holder, id(1)).await, "fiat-sent");
        assert_eq!(holder.fake.received().len(), 1, "never retried");
    }

    #[tokio::test]
    async fn an_order_with_no_local_row_or_key_sends_nothing() {
        let holder = holder(buying, 0, |respond| respond).await;
        let before = database(&holder);

        let unknown = take(&holder, StepKind::FiatSent, id(3)).await;

        assert_eq!(unknown.status(), Outcome::Error);
        assert_eq!(code(&unknown), Some(ErrorCode::OrderUnknown));
        assert_eq!(
            stages(&unknown),
            meta("not_attempted", "unknown", "not_attempted")
        );
        assert!(holder.fake.received().is_empty());
        assert_eq!(database(&holder), before);

        sql(&holder, "UPDATE orders SET trade_keys = NULL").await;
        for kind in [StepKind::FiatSent, StepKind::Release, StepKind::Cancel] {
            let keyless = take(&holder, kind, id(1)).await;
            assert_eq!(code(&keyless), Some(ErrorCode::OrderUnknown), "{kind:?}");
        }
        assert!(holder.fake.received().is_empty());
    }

    #[tokio::test]
    async fn work_the_node_expects_stops_every_step_before_anything_is_sent() {
        let holder = holder(buying, 9, |respond| respond).await;
        let before = database(&holder);

        for kind in [StepKind::FiatSent, StepKind::Release, StepKind::Cancel] {
            let outcome = take(&holder, kind, id(1)).await;
            assert_eq!(outcome.status(), Outcome::Error, "{kind:?}");
            assert_eq!(code(&outcome), Some(ErrorCode::PowRequired), "{kind:?}");
            assert_eq!(
                stages(&outcome),
                meta("not_attempted", "unknown", "not_attempted")
            );
        }
        assert!(holder.fake.received().is_empty());
        assert_eq!(database(&holder), before);
    }

    const RANGE: &str =
        "UPDATE orders SET is_mine = 1, min_amount = 10, max_amount = 100, fiat_amount = 40";

    #[tokio::test]
    async fn a_range_order_with_room_left_is_accepted_but_not_continued_and_says_so() {
        for (kind, trades) in [
            (
                StepKind::FiatSent,
                Box::new(buying) as Box<dyn FnOnce(PublicKey, PublicKey) -> Vec<Traded>>,
            ),
            (StepKind::Release, Box::new(selling_at(Status::FiatSent))),
        ] {
            let holder = holder(trades, 0, |respond| respond).await;
            sql(&holder, RANGE).await;
            let index = last_index(&holder).await;

            let outcome = take(&holder, kind, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Ok, "{kind:?}");
            assert_eq!(warnings(&outcome), ["range_not_continued"], "{kind:?}");
            assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
            assert_eq!(last_index(&holder).await, index, "no index is reserved");
            assert_eq!(holder.fake.received()[0].next_trade, None);
        }
    }

    #[tokio::test]
    async fn no_range_warning_for_the_taker_of_a_range_order() {
        let holder = self::holder(buying, 0, |respond| respond).await;
        sql(
            &holder,
            "UPDATE orders SET is_mine = 0, min_amount = 10, max_amount = 100, fiat_amount = 40",
        )
        .await;

        let outcome = take(&holder, StepKind::FiatSent, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert!(warnings(&outcome).is_empty(), "a taker continues nothing");
    }

    #[tokio::test]
    async fn no_range_warning_for_a_fixed_order_an_exhausted_range_or_a_cancel() {
        let holder = self::holder(buying, 0, |respond| respond).await;
        let fixed = take(&holder, StepKind::FiatSent, id(1)).await;
        assert!(warnings(&fixed).is_empty(), "fixed");

        let holder = self::holder(buying, 0, |respond| respond).await;
        sql(
            &holder,
            "UPDATE orders SET is_mine = 1, min_amount = 10, max_amount = 100, fiat_amount = 95",
        )
        .await;
        let exhausted = take(&holder, StepKind::FiatSent, id(1)).await;
        assert!(warnings(&exhausted).is_empty(), "exhausted");

        let holder = self::holder(buying, 0, |respond| respond).await;
        sql(&holder, RANGE).await;
        let canceled = take(&holder, StepKind::Cancel, id(1)).await;
        assert_eq!(canceled.status(), Outcome::Ok);
        assert!(warnings(&canceled).is_empty(), "a cancel continues nothing");
        let _ = &holder.other;
    }

    // --- The binary (slice 4b) ---

    use harness::{run_with_watchdog, Captured, WATCHDOG};
    use std::process::Stdio;

    const ORDER: &str = "00000000-0000-0000-0000-000000000001";
    const STEPS: [(StepKind, &str); 3] = [
        (StepKind::FiatSent, "fiat-sent"),
        (StepKind::Release, "release"),
        (StepKind::Cancel, "cancel"),
    ];

    /// A holder in the state the step needs to be accepted.
    async fn ready(kind: StepKind) -> Holder {
        match kind {
            StepKind::Release => holder(selling_at(Status::FiatSent), 0, |respond| respond).await,
            _ => holder(buying, 0, |respond| respond).await,
        }
    }

    /// `<front> order <step> --order ORDER <tail>`.
    fn step_args<'a>(front: &[&'a str], step: &'a str, tail: &[&'a str]) -> Vec<&'a str> {
        let mut all: Vec<&str> = front.to_vec();
        all.extend(["order", step, "--order", ORDER]);
        all.extend(tail);
        all
    }

    fn command(holder: &Holder, args: &[&str]) -> tokio::process::Command {
        let mut command = holder.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", holder.config.mostro_pubkey().to_hex())
            .env("RELAYS", &holder._relay.url);
        command
    }

    async fn run(holder: &Holder, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(holder, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    /// Run with `answer` as the whole of stdin: a file the test wrote.
    async fn run_answering(holder: &Holder, args: &[&str], answer: &str) -> Captured {
        let path = std::env::temp_dir().join(format!(
            "mcli-step-answer-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::write(&path, answer).unwrap();
        let mut command = command(holder, args);
        command.stdin(Stdio::from(std::fs::File::open(&path).unwrap()));
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    fn document(run: &Captured) -> Value {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        serde_json::from_str(&stdout).unwrap()
    }

    fn snapshot(holder: &Holder) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(holder.sandbox.mcli_dir())
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

    fn conforms_to_the_checked_in_schema(step: &str, outcome: &Value) {
        let path = format!(
            "{}/docs/schemas/order-{step}-v1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    #[tokio::test]
    async fn each_step_in_json_is_accepted_and_nothing_secret_is_shown() {
        let expected = [
            ("fiat_sent", "fiat-sent", Some("wait_for_release")),
            ("released", "settled-hold-invoice", None),
            ("cancel_requested", "active", Some("wait_for_peer_cancel")),
        ];
        for ((kind, step), (result, local, next)) in STEPS.into_iter().zip(expected) {
            let holder = ready(kind).await;
            let index = last_index(&holder).await;

            let run = run(
                &holder,
                &step_args(&["--output", "json", "--verbose"], step, &["--yes"]),
            )
            .await;

            assert_eq!(run.code(), Some(0), "{step}: {}", run.stderr_text());
            assert_eq!(
                run.stderr_text(),
                format!("verbose: order.{step} ok\n"),
                "{step}"
            );
            let outcome = document(&run);
            conforms_to_the_checked_in_schema(step, &outcome);
            assert_eq!(outcome["command"], format!("order.{step}"));
            assert_eq!(outcome["status"], "ok");
            assert_eq!(outcome["data"]["result"], result, "{step}");
            assert_eq!(outcome["data"]["order_id"], ORDER);
            assert_eq!(outcome["data"]["next_trade_index"], Value::Null);
            assert_eq!(
                outcome["data"]["next_step"],
                next.map_or(Value::Null, Value::from)
            );
            assert_eq!(outcome["meta"]["publication"], "published", "{step}");
            assert_eq!(status_of(&holder, id(1)).await, local, "{step}");
            assert_eq!(last_index(&holder).await, index, "no trade index is spent");
            assert_eq!(holder.fake.received().len(), 1, "{step}");
            let secret = holder.me.secret_key().to_secret_hex();
            for stream in [run.stdout_text(), run.stderr_text()] {
                assert!(!stream.contains(&secret), "{step}: the trade key leaked");
                assert!(!stream.contains("127.0.0.1"), "{step}: a relay leaked");
            }
        }
    }

    #[tokio::test]
    async fn json_without_yes_does_nothing_for_every_step() {
        for (kind, step) in STEPS {
            let holder = ready(kind).await;
            let before = snapshot(&holder);

            let run = run(&holder, &step_args(&["--output", "json"], step, &[])).await;

            assert_eq!(run.code(), Some(2), "{step}: {}", run.stderr_text());
            assert!(run.stderr.is_empty());
            let outcome = document(&run);
            conforms_to_the_checked_in_schema(step, &outcome);
            assert_eq!(outcome["error"]["code"], "invalid_arguments");
            assert!(holder.fake.received().is_empty());
            assert_eq!(snapshot(&holder), before);
        }
    }

    #[tokio::test]
    async fn the_prompt_accepts_y_and_blank_and_declines_everything_else() {
        let titles = ["Fiat Marked As Sent", "Sats Released", "Cancel Requested"];
        let previews = ["Fiat Sent Preview", "Release Preview", "Cancel Preview"];
        for (((kind, step), title), preview) in STEPS.into_iter().zip(titles).zip(previews) {
            for answer in ["y\n", "\n", "Y\r\n"] {
                let holder = ready(kind).await;
                let run = run_answering(&holder, &step_args(&[], step, &[]), answer).await;
                assert_eq!(
                    run.code(),
                    Some(0),
                    "{step} {answer:?}: {}",
                    run.stderr_text()
                );
                let out = run.stdout_text();
                assert!(out.contains(preview) && out.contains(title), "{out}");
                assert!(run.stderr_text().contains("Send this request? [Y/n]"));
                assert_eq!(holder.fake.received().len(), 1, "{step} {answer:?}");
            }
            for answer in ["n\n", "no\n", "maybe\n", ""] {
                let holder = ready(kind).await;
                let before = snapshot(&holder);
                let run = run_answering(&holder, &step_args(&[], step, &[]), answer).await;
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{step} {answer:?}: {}",
                    run.stderr_text()
                );
                assert!(!run.stdout_text().contains(title));
                assert!(run
                    .stderr_text()
                    .contains("error: The order was not confirmed, so nothing was sent."));
                assert!(holder.fake.received().is_empty(), "{step} {answer:?}");
                assert_eq!(snapshot(&holder), before, "{step} {answer:?}");
            }
            let holder = ready(kind).await;
            let closed = self::run(&holder, &step_args(&[], step, &[])).await;
            assert_eq!(closed.code(), Some(1), "a closed stdin is not approval");
        }
    }

    #[tokio::test]
    async fn the_release_preview_warns_before_the_question() {
        let holder = ready(StepKind::Release).await;

        let run = run_answering(&holder, &step_args(&[], "release", &[]), "n\n").await;

        assert!(run.stdout_text().contains("cannot be undone"));
    }

    #[tokio::test]
    async fn an_unknown_order_fails_before_any_question_is_asked() {
        let other = "00000000-0000-0000-0000-000000000003";
        for (kind, step) in STEPS {
            let holder = ready(kind).await;
            let before = snapshot(&holder);

            let human = run(&holder, &["order", step, "--order", other]).await;
            assert_eq!(human.code(), Some(1), "{step}");
            assert!(human.stdout.is_empty());
            assert_eq!(
                human.stderr_text(),
                "error: No local order has this id, so nothing was sent.\n"
            );
            let json = run(
                &holder,
                &["--output", "json", "order", step, "--order", other, "--yes"],
            )
            .await;
            assert_eq!(json.code(), Some(1));
            let outcome = document(&json);
            conforms_to_the_checked_in_schema(step, &outcome);
            assert_eq!(outcome["error"]["code"], "order_unknown");
            assert!(holder.fake.received().is_empty());
            assert_eq!(snapshot(&holder), before);
        }
    }

    #[tokio::test]
    async fn a_refusal_shows_its_reason_in_both_modes_and_changes_nothing() {
        // The seller cannot say fiat was sent.
        let holder = holder(selling_at(Status::Active), 0, |respond| respond).await;
        let before = snapshot(&holder);

        let json = run(
            &holder,
            &step_args(&["--output", "json"], "fiat-sent", &["--yes"]),
        )
        .await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("fiat-sent", &outcome);
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], "invalid_pubkey");
        assert_eq!(outcome["data"], Value::Null);

        let human = run(&holder, &step_args(&[], "fiat-sent", &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: The node refused the request. (reason: invalid_pubkey)\n"
        );
        assert_eq!(snapshot(&holder), before);
        assert_eq!(holder.fake.received().len(), 2, "one request per run");
    }

    #[tokio::test]
    async fn a_silent_node_is_uncertain_after_the_production_budget_and_is_never_retried() {
        let holder = holder(buying, 0, |_| fake_mostro::silent()).await;
        let started = std::time::Instant::now();

        let json = run(
            &holder,
            &step_args(&["--output", "json"], "release", &["--yes"]),
        )
        .await;

        assert!(started.elapsed() >= ExchangeLimits::default().reply);
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        assert!(json.stderr.is_empty());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("release", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
        assert_eq!(outcome["meta"]["persistence"], "not_attempted");
        assert_eq!(holder.fake.received().len(), 1, "never retried");

        let human = run(&holder, &step_args(&[], "release", &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        let err = human.stderr_text();
        assert!(
            err.contains("error: The node did not answer before the deadline."),
            "{err}"
        );
        assert!(
            err.contains("The node may have released the sats. Check `order info --order <id>`"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_range_order_with_room_left_warns_in_both_modes_and_the_exit_is_success() {
        let holder = holder(buying, 0, |respond| respond).await;
        sql(&holder, RANGE).await;

        let json = run(
            &holder,
            &step_args(&["--output", "json"], "fiat-sent", &["--yes"]),
        )
        .await;
        assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("fiat-sent", &outcome);
        assert_eq!(outcome["warnings"][0]["code"], "range_not_continued");

        let holder = self::holder(buying, 0, |respond| respond).await;
        sql(&holder, RANGE).await;
        let human = run(&holder, &step_args(&[], "fiat-sent", &["--yes"])).await;
        assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
        assert!(human.stdout_text().contains("Fiat Marked As Sent"));
        assert!(
            human
                .stderr_text()
                .contains("warning: this is a range order with room left"),
            "{}",
            human.stderr_text()
        );
    }

    #[tokio::test]
    async fn a_stale_local_status_is_a_warning_and_the_exit_is_still_success() {
        let holder = ready(StepKind::Release).await;
        sql(
            &holder,
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .await;

        let json = run(
            &holder,
            &step_args(&["--output", "json"], "release", &["--yes"]),
        )
        .await;
        assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("release", &outcome);
        assert_eq!(outcome["meta"]["persistence"], "failed");
        assert_eq!(outcome["warnings"][0]["code"], "local_status_stale");

        let holder = ready(StepKind::Release).await;
        sql(
            &holder,
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .await;
        let human = run(&holder, &step_args(&[], "release", &["--yes"])).await;
        assert_eq!(human.code(), Some(0));
        assert!(human
            .stderr_text()
            .contains("warning: the node accepted the request but the local order status"));
    }

    #[tokio::test]
    async fn the_two_cooperative_cancel_steps_complete_a_cancel_from_the_binary() {
        let holder = holder(
            |me, other| {
                let mut asked = Traded::active(id(1), other, me);
                asked.initiator = Some(other);
                vec![asked]
            },
            0,
            |respond| respond,
        )
        .await;

        let run = run(
            &holder,
            &step_args(&["--output", "json"], "cancel", &["--yes"]),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("cancel", &outcome);
        assert_eq!(outcome["data"]["result"], "cancel_accepted");
        assert_eq!(status_of(&holder, id(1)).await, "cooperatively-canceled");
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
        ReadOnly,
    }

    async fn tweak(holder: &Holder, how: Tweak) {
        let path = holder.sandbox.mcli_dir().join("mcli.db");
        let statement = |sql: &'static str| {
            let path = path.clone();
            async move {
                let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
                let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
                sqlx::query(sql).execute(&pool).await.unwrap();
                pool.close().await;
            }
        };
        match how {
            Tweak::None => {}
            Tweak::Absent => std::fs::remove_file(&path).unwrap(),
            Tweak::Garbage => std::fs::write(&path, b"this is not a sqlite database").unwrap(),
            Tweak::EmptyUsers => statement("DELETE FROM users").await,
            Tweak::NoUsersTable => statement("DROP TABLE users").await,
            Tweak::NullMnemonic => statement("UPDATE users SET mnemonic = NULL").await,
            Tweak::ReadOnly => {
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&path, permissions).unwrap();
            }
        }
    }

    /// Whether the process can still write a file marked read-only (root).
    fn ignores_read_only(holder: &Holder) -> bool {
        std::fs::OpenOptions::new()
            .write(true)
            .open(holder.sandbox.mcli_dir().join("mcli.db"))
            .is_ok()
    }

    struct Failing {
        label: &'static str,
        tweak: Tweak,
        set: &'static [(&'static str, &'static str)],
        unset: &'static [&'static str],
        /// Global flags placed before the command.
        flags: &'static [&'static str],
        code: &'static str,
    }

    impl Failing {
        const fn new(label: &'static str, tweak: Tweak, code: &'static str) -> Self {
            Self {
                label,
                tweak,
                set: &[],
                unset: &[],
                flags: &[],
                code,
            }
        }
    }

    async fn run_env(
        holder: &Holder,
        args: &[&str],
        set: &[(&str, &str)],
        unset: &[&str],
    ) -> Captured {
        let mut command = command(holder, args);
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

    /// Run `case` as JSON, human and verbose, on every step, and check what
    /// every mode shows, then that nothing was sent and nothing changed.
    async fn check_failure(case: &Failing, holder: &Holder) {
        tweak(holder, case.tweak).await;
        let before = snapshot(holder);
        let label = case.label;
        for (_, step) in STEPS {
            for mode in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
                let mut front: Vec<&str> = case.flags.to_vec();
                front.extend(mode);
                let run = run_env(
                    holder,
                    &step_args(&front, step, &["--yes"]),
                    case.set,
                    case.unset,
                )
                .await;
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{label} {step} {mode:?}: {}",
                    run.stderr_text()
                );
                let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
                for stream in [&stdout, &stderr] {
                    assert!(!stream.contains("SENTINEL"), "{label}: a sentinel leaked");
                    assert!(
                        !stream.contains("127.0.0.1"),
                        "{label}: a relay address leaked"
                    );
                }
                if mode == ["--output", "json"] {
                    let outcome = document(&run);
                    conforms_to_the_checked_in_schema(step, &outcome);
                    assert_eq!(outcome["command"], format!("order.{step}"), "{label}");
                    assert_eq!(outcome["status"], "error", "{label}");
                    assert_eq!(outcome["data"], Value::Null, "{label}");
                    assert_eq!(outcome["error"]["code"], case.code, "{label}");
                    assert_eq!(
                        outcome["meta"],
                        json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                               "persistence": "not_attempted", "snapshot": null}),
                        "{label}"
                    );
                    assert!(stderr.is_empty(), "{label}: {stderr}");
                } else {
                    assert!(stdout.is_empty(), "{label} {mode:?}: {stdout}");
                    assert!(stderr.starts_with("error: "), "{label}: {stderr}");
                    assert!(
                        !stderr.contains("may have"),
                        "{label}: not uncertain: {stderr}"
                    );
                }
            }
        }
        assert!(
            holder.fake.received().is_empty(),
            "{label}: something was sent"
        );
        if !matches!(case.tweak, Tweak::ReadOnly) {
            assert_eq!(snapshot(holder), before, "{label}: the store changed");
        }
        assert!(
            harness::is_empty_dir(&holder.sandbox.home())
                && harness::is_empty_dir(&holder.sandbox.cwd()),
            "{label}: files were created elsewhere"
        );
    }

    #[tokio::test]
    async fn failures_before_any_request_are_stable_and_leave_no_trace() {
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
            Failing {
                flags: &["--pow", SENTINEL],
                ..Failing::new("bad pow", Tweak::None, "configuration_invalid")
            },
            Failing {
                flags: &["--transport", SENTINEL],
                ..Failing::new("bad transport", Tweak::None, "configuration_invalid")
            },
        ];
        for case in &cases {
            let holder = ready(StepKind::FiatSent).await;
            check_failure(case, &holder).await;
        }
    }

    #[tokio::test]
    async fn every_step_needs_a_writable_store() {
        let holder = ready(StepKind::FiatSent).await;
        tweak(&holder, Tweak::ReadOnly).await;
        if ignores_read_only(&holder) {
            return; // running as a user the file mode does not bind
        }
        check_failure(
            &Failing::new("read-only store", Tweak::None, "store_unwritable"),
            &holder,
        )
        .await;
    }

    #[tokio::test]
    async fn a_node_that_cannot_be_served_fails_before_publishing() {
        let old = holder_with(buying, (1, 0, false), |respond| respond).await;
        check_failure(
            &Failing::new("protocol 1", Tweak::None, "protocol_unsupported"),
            &old,
        )
        .await;

        let work = holder_with(buying, (2, 9, false), |respond| respond).await;
        check_failure(
            &Failing::new("more work expected", Tweak::None, "pow_required"),
            &work,
        )
        .await;

        // An explicit transport skips the protocol check and enough work passes.
        let holder = holder_with(buying, (1, 9, false), |respond| respond).await;
        let run = run(
            &holder,
            &step_args(
                &["--transport", "nip44", "--pow", "9", "--output", "json"],
                "fiat-sent",
                &["--yes"],
            ),
        )
        .await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    }

    #[tokio::test]
    async fn a_relay_that_rejects_the_request_changes_nothing() {
        let holder = holder_with(buying, (2, 0, true), |respond| respond).await;
        check_failure(
            &Failing::new("rejected", Tweak::None, "request_rejected"),
            &holder,
        )
        .await;
    }

    #[tokio::test]
    async fn an_unreachable_relay_is_a_connection_failure_and_costs_nothing() {
        let holder = ready(StepKind::Cancel).await;
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);
        let before = snapshot(&holder);

        let run = run_env(
            &holder,
            &step_args(&["--output", "json"], "cancel", &["--yes"]),
            &[("RELAYS", dead.as_str())],
            &[],
        )
        .await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "relay_connection_failed");
        assert_eq!(outcome["status"], "error");
        assert_eq!(outcome["meta"]["publication"], "not_attempted");
        assert!(!run.stderr_text().contains("127.0.0.1"));
        assert_eq!(snapshot(&holder), before);
    }

    #[tokio::test]
    async fn invalid_input_is_an_argument_error_before_any_effect() {
        let holder = ready(StepKind::FiatSent).await;
        let before = snapshot(&holder);
        let mut cases: Vec<(Vec<&str>, Value)> = Vec::new();
        for (_, step) in STEPS {
            let command = json!(format!("order.{step}"));
            for tail in [
                vec!["--order", "not-a-uuid", "--yes"],
                vec!["--yes"],
                vec!["--order", ORDER, "--yes", "--invoice", SENTINEL],
                vec!["--order-id", ORDER, "--yes"],
                vec!["-o", ORDER, "--yes"],
                vec!["--orderid", ORDER, "--yes"],
            ] {
                let mut all = vec!["--output", "json", "order", step];
                all.extend(tail);
                cases.push((all, command.clone()));
            }
        }
        // The removed spellings have no command.
        for name in ["fiatsent", "release", "cancel"] {
            cases.push((vec!["--output", "json", name, "-o", ORDER], Value::Null));
        }
        for (arguments, command) in cases {
            let run = run(&holder, &arguments).await;
            assert_eq!(run.code(), Some(2), "{arguments:?}: {}", run.stderr_text());
            let outcome = document(&run);
            assert_eq!(outcome["command"], command, "{arguments:?}");
            assert_eq!(
                outcome["error"]["code"], "invalid_arguments",
                "{arguments:?}"
            );
            assert_eq!(
                outcome["meta"]["publication"], "not_attempted",
                "{arguments:?}"
            );
            for text in [run.stdout_text(), run.stderr_text()] {
                assert!(!text.contains("SENTINEL"), "{arguments:?}: {text}");
            }
            assert!(
                run.stderr.is_empty(),
                "{arguments:?}: {}",
                run.stderr_text()
            );
        }
        assert!(holder.fake.received().is_empty());
        assert_eq!(snapshot(&holder), before);
    }

    #[tokio::test]
    async fn an_unwritable_stdout_after_the_step_exits_1_with_one_fixed_line() {
        for (front, line) in [
            (
                &["--output", "json"][..],
                "error: the JSON outcome could not be written\n",
            ),
            (&[][..], "error: the result could not be written\n"),
        ] {
            for (kind, step) in STEPS {
                let holder = ready(kind).await;
                let all = step_args(front, step, &["--yes"]);
                let mut command = command(&holder, &all);
                command.stdout(harness::closed_pipe());
                let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{step} {front:?}: {}",
                    run.stderr_text()
                );
                assert_eq!(run.stderr_text(), line, "{step} {front:?}");
                // The request was sent and the node acted all the same.
                assert_eq!(holder.fake.received().len(), 1, "{step}");
            }
        }
    }

    #[tokio::test]
    async fn an_unwritable_stderr_changes_neither_the_exit_nor_the_document() {
        let holder = ready(StepKind::FiatSent).await;
        let mut command = command(
            &holder,
            &step_args(&["--output", "json", "--pow", "x"], "fiat-sent", &["--yes"]),
        );
        command.stderr(harness::closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1));
        assert_eq!(document(&run)["error"]["code"], "configuration_invalid");
    }

    #[tokio::test]
    async fn no_secret_reaches_a_stream_on_a_successful_run() {
        // The order id, the trade key and the node key are public data; the
        // secrets of the trade key and the identity never are.
        for front in [&["--output", "json", "--verbose"][..], &["--verbose"][..]] {
            let holder = ready(StepKind::Release).await;
            let run = run(&holder, &step_args(front, "release", &["--yes"])).await;
            assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
            for stream in [run.stdout_text(), run.stderr_text()] {
                assert!(!stream.contains(&holder.me.secret_key().to_secret_hex()));
                assert!(!stream.contains(&holder.identity.secret_key().to_secret_hex()));
                assert!(!stream.contains("127.0.0.1"));
            }
        }
    }
}
