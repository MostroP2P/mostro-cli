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

use fake_mostro::{info_event, step_daemon, FakeMostro, Respond, Traded};
use harness::{Backend, LoopbackRelay};

const REQUEST_ID: u64 = 7070;
const WAIT: Duration = Duration::from_millis(700);

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
