//! Tests for `order add-invoice` and `bond add-invoice` (spec 005).
//!
//! Slice 1 holds the self-tests of the fixtures built for the commands: the
//! scripted daemon that applies the source's `add_invoice_action` and
//! `add_bond_invoice_action` rules. The commands' own scenarios are added with
//! the slices that implement them.

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

use fake_mostro::{
    info_event, invoice_daemon, payout_invoice, BondOwed, FakeMostro, Ledger, Owed, Respond,
};
use harness::{Backend, LoopbackRelay};

const REQUEST_ID: u64 = 7150;
const WAIT: Duration = Duration::from_millis(700);
const INVOICE: &str = "lnbcrt1scriptedbuyerinvoice";

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

    async fn hear(&self, key: &Keys, act: impl std::future::Future<Output = ()>) -> Vec<Message> {
        self.hear_all(&[key], act).await.remove(0)
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

fn add_invoice(order: Uuid, invoice: Option<&str>) -> Message {
    Message::new_order(
        Some(order),
        Some(REQUEST_ID),
        None,
        Action::AddInvoice,
        invoice.map(|invoice| Payload::PaymentRequest(None, invoice.to_string(), None)),
    )
}

fn add_bond_invoice(order: Uuid, invoice: &str) -> Message {
    Message::new_order(
        Some(order),
        Some(REQUEST_ID),
        None,
        Action::AddBondInvoice,
        Some(Payload::PaymentRequest(None, invoice.to_string(), None)),
    )
}

struct World {
    _relay: LoopbackRelay,
    fake: FakeMostro,
    peer: Peer,
    ledger: Arc<Mutex<Ledger>>,
    buyer: Keys,
}

async fn world(orders: impl FnOnce(PublicKey) -> Vec<Owed>, acks: bool, share: i64) -> World {
    let buyer = Keys::generate();
    let ledger = Ledger {
        orders: orders(buyer.public_key()),
        bonds: vec![BondOwed {
            order: id(9),
            recipient: buyer.public_key(),
            share,
            open: true,
            invoice: None,
        }],
    };
    let (respond, ledger): (Respond, _) = invoice_daemon(ledger, acks);
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
        ledger,
        buyer,
    }
}

impl World {
    /// Answer as `trade`; the replies it hears.
    async fn answer_as(&self, trade: &Keys, message: Message) -> Vec<Message> {
        let identity = Keys::generate();
        self.peer
            .hear(trade, self.peer.send(&message, &identity, trade))
            .await
    }

    async fn answer(&self, message: Message) -> Vec<Message> {
        self.answer_as(&self.buyer, message).await
    }
}

fn only(replies: Vec<Message>) -> MessageKind {
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies[0].get_inner_message_kind().clone()
}

fn reason(kind: &MessageKind) -> CantDoReason {
    assert_eq!(kind.action, Action::CantDo);
    match &kind.payload {
        Some(Payload::CantDo(Some(reason))) => reason.clone(),
        other => panic!("a refusal, got {other:?}"),
    }
}

fn waiting(buyer: PublicKey) -> Vec<Owed> {
    vec![Owed::waiting(id(1), buyer)]
}

#[tokio::test]
async fn an_invoice_for_a_waiting_order_is_answered_with_waiting_seller_to_pay() {
    let world = world(waiting, true, 5_000).await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::WaitingSellerToPay);
    assert!(reply.payload.is_none(), "no payload, as in the source");
    assert_eq!(reply.id, Some(id(1)), "the message id is the order id");
    assert_eq!(
        reply.request_id,
        Some(REQUEST_ID),
        "the request id is echoed"
    );
    let ledger = world.ledger.lock().unwrap();
    assert_eq!(ledger.orders[0].status, Status::WaitingPayment);
    assert_eq!(ledger.orders[0].invoice.as_deref(), Some(INVOICE));
    let seen = world.fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].trade_index, None,
        "an invoice carries no trade index"
    );
    assert_eq!(seen[0].invoice.as_deref(), Some(INVOICE));
    assert_eq!(seen[0].author, world.buyer.public_key());
}

#[tokio::test]
async fn an_order_whose_seller_already_paid_goes_active_with_the_order_in_the_reply() {
    let world = world(
        |buyer| {
            vec![Owed {
                preimage: true,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::HoldInvoicePaymentAccepted);
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(
        (order.id, order.status),
        (Some(id(1)), Some(Status::Active))
    );
    assert_eq!(
        world.ledger.lock().unwrap().orders[0].status,
        Status::Active
    );
}

#[tokio::test]
async fn a_replacement_invoice_on_a_settled_order_is_updated_without_a_payload() {
    let world = world(
        |buyer| {
            vec![Owed {
                status: Status::SettledHoldInvoice,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::InvoiceUpdated);
    assert!(reply.payload.is_none());
    assert_eq!(reply.request_id, Some(REQUEST_ID));
    let ledger = world.ledger.lock().unwrap();
    assert_eq!(ledger.orders[0].status, Status::SettledHoldInvoice);
    assert_eq!(ledger.orders[0].invoice.as_deref(), Some(INVOICE));
}

#[tokio::test]
async fn every_add_invoice_refusal_leaves_the_order_untouched() {
    let stranger = Keys::generate();
    let cases: Vec<(&str, Message, bool, CantDoReason)> = vec![
        (
            "an unknown order",
            add_invoice(id(2), Some(INVOICE)),
            false,
            CantDoReason::NotFound,
        ),
        (
            "another key",
            add_invoice(id(1), Some(INVOICE)),
            true,
            CantDoReason::InvalidPeer,
        ),
        (
            "no invoice",
            add_invoice(id(1), None),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "a bad invoice",
            add_invoice(id(1), Some("not an invoice")),
            false,
            CantDoReason::InvalidInvoice,
        ),
    ];
    for (name, message, as_stranger, expected) in cases {
        let world = world(waiting, true, 5_000).await;
        let replies = if as_stranger {
            world.answer_as(&stranger, message).await
        } else {
            world.answer(message).await
        };
        assert_eq!(reason(&only(replies)), expected, "{name}");
        let ledger = world.ledger.lock().unwrap();
        assert_eq!(
            ledger.orders[0].status,
            Status::WaitingBuyerInvoice,
            "{name}"
        );
        assert_eq!(ledger.orders[0].invoice, None, "{name}");
    }
    let world = world(
        |buyer| {
            vec![Owed {
                status: Status::Active,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;
    let kind = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);
    assert_eq!(reason(&kind), CantDoReason::NotAllowedByStatus);
    assert_eq!(world.ledger.lock().unwrap().orders[0].invoice, None);
}

#[tokio::test]
async fn an_accepted_bond_invoice_is_acknowledged_without_a_request_id() {
    let world = world(waiting, true, 5_000).await;

    let reply = only(
        world
            .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
            .await,
    );

    assert_eq!(reply.action, Action::BondInvoiceAccepted);
    assert_eq!(
        reply.request_id, None,
        "the source sets no request id on this ack"
    );
    assert_eq!(reply.id, Some(id(9)));
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(order.amount, 5_000, "the counterparty share");
    let ledger = world.ledger.lock().unwrap();
    assert!(!ledger.bonds[0].open);
    assert_eq!(ledger.bonds[0].invoice, Some(payout_invoice(5_000)));
    assert_eq!(world.fake.received()[0].trade_index, None);
}

#[tokio::test]
async fn an_older_node_accepts_a_bond_invoice_in_silence() {
    let world = world(waiting, false, 5_000).await;

    let replies = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;

    assert!(replies.is_empty(), "{replies:?}");
    assert!(
        !world.ledger.lock().unwrap().bonds[0].open,
        "but it was accepted"
    );
}

#[tokio::test]
async fn every_bond_invoice_refusal_leaves_the_bond_open() {
    let stranger = Keys::generate();
    let cases: Vec<(&str, Message, bool, CantDoReason)> = vec![
        (
            "a wrong amount",
            add_bond_invoice(id(9), &payout_invoice(4_999)),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "a Lightning address",
            add_bond_invoice(id(9), "buyer@example.com"),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "an order with no bond",
            add_bond_invoice(id(1), &payout_invoice(5_000)),
            false,
            CantDoReason::NotAllowedByStatus,
        ),
        (
            "another key",
            add_bond_invoice(id(9), &payout_invoice(5_000)),
            true,
            CantDoReason::NotAllowedByStatus,
        ),
    ];
    for (name, message, as_stranger, expected) in cases {
        let world = world(waiting, true, 5_000).await;
        let replies = if as_stranger {
            world.answer_as(&stranger, message).await
        } else {
            world.answer(message).await
        };
        assert_eq!(reason(&only(replies)), expected, "{name}");
        let ledger = world.ledger.lock().unwrap();
        assert!(ledger.bonds[0].open, "{name}");
        assert_eq!(ledger.bonds[0].invoice, None, "{name}");
    }
}

#[tokio::test]
async fn a_second_bond_invoice_for_a_paid_out_bond_is_refused() {
    let world = world(waiting, true, 5_000).await;
    let first = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;
    assert_eq!(only(first).action, Action::BondInvoiceAccepted);

    let second = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;

    assert_eq!(reason(&only(second)), CantDoReason::NotAllowedByStatus);
}

#[tokio::test]
async fn any_other_action_is_refused() {
    let world = world(waiting, true, 5_000).await;
    let message = Message::new_order(Some(id(1)), Some(REQUEST_ID), None, Action::Release, None);

    let kind = only(world.answer(message).await);

    assert_eq!(reason(&kind), CantDoReason::InvalidAction);
}
