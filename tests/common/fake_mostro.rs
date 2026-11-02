//! A scripted fake Mostro daemon and identity-store seeding for the order-info
//! tests. Included with `#[path]` next to `harness.rs`; each test crate uses a
//! different subset.
//!
//! The daemon is a normal Nostr client holding generated node keys. It
//! subscribes to the kind-14 messages addressed to the node on a loopback
//! relay, decrypts each request with `unwrap_incoming`, records it, and
//! publishes whatever its script returns. It never contacts anything else.
#![allow(dead_code)]

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, Payload, Peer, Transport,
    WrapOptions, NOSTR_INFO_EVENT_KIND,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, EventBuilder, EventId, Filter, FinalizeEvent, Keys, Kind,
    PublicKey, Tag, Timestamp,
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
    /// The trade index the message carries.
    pub trade_index: Option<i64>,
    /// The order of an `Order` payload.
    pub order: Option<SmallOrder>,
    /// Whether the inner signature verifies against the author (false when absent).
    pub signature_valid: bool,
    /// The order id the message names (a take names the order it takes).
    pub order_id: Option<Uuid>,
    /// The amount a take carries, as `Amount` or inside a payment request.
    pub amount: Option<i64>,
    /// The invoice a take carries.
    pub invoice: Option<String>,
    /// The next trade key and index a `fiat-sent` or `release` carries.
    pub next_trade: Option<(String, u32)>,
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

/// The acknowledgement of a `new-order`: the order the daemon created from the
/// request, with `id`, echoing the request id and trade index.
pub fn order_created(request: &Received, id: Uuid) -> Message {
    let mut order = request.order.clone().expect("a new-order carries an order");
    order.id = Some(id);
    Message::new_order(
        Some(id),
        request.request_id,
        request.trade_index,
        Action::NewOrder,
        Some(Payload::Order(order)),
    )
}

/// What a bonded node answers instead: a bond invoice to pay first.
pub fn bond_requested(request: &Received, id: Uuid) -> Message {
    let mut order = request.order.clone().expect("a new-order carries an order");
    order.id = Some(id);
    Message::new_order(
        Some(id),
        request.request_id,
        request.trade_index,
        Action::PayBondInvoice,
        Some(Payload::PaymentRequest(
            Some(order),
            "lnbcrt1bondinvoice".to_string(),
            None,
        )),
    )
}

/// The daemon's checks on a `new-order`, in the order of its source
/// (`check_trade_index`, then `order_action`), and the index it remembers.
/// Returns the script and a handle on `last_trade_index`.
pub fn new_order_daemon(last_trade_index: i64, bonded: bool) -> (Respond, Arc<Mutex<i64>>) {
    let last = Arc::new(Mutex::new(last_trade_index));
    let state = Arc::clone(&last);
    let respond: Respond = Arc::new(move |request: &Received| {
        let refuse = |reason| vec![Outgoing::to_requester(refusal(request, reason))];
        if request.action != Action::NewOrder {
            return refuse(CantDoReason::InvalidAction);
        }
        let index = request.trade_index.unwrap_or(0);
        let mut last = state.lock().unwrap();
        if index <= *last || request.author == request.identity {
            return refuse(CantDoReason::InvalidTradeIndex);
        }
        if !request.signature_valid {
            return refuse(CantDoReason::InvalidSignature);
        }
        let Some(order) = request.order.as_ref() else {
            return refuse(CantDoReason::InvalidParameters);
        };
        if order.min_amount.is_some() != order.max_amount.is_some() {
            return refuse(CantDoReason::InvalidAmount);
        }
        if order.min_amount.is_none() && order.fiat_amount <= 0 {
            return refuse(CantDoReason::InvalidAmount);
        }
        // The index is remembered only after every check passed.
        *last = index;
        let id = Uuid::new_v4();
        let reply = if bonded {
            bond_requested(request, id)
        } else {
            order_created(request, id)
        };
        vec![Outgoing::to_requester(reply)]
    });
    (respond, last)
}

// --- Takes (spec 004) ---------------------------------------------------------

/// The hold invoice the scripted node returns to a seller.
pub const HOLD_INVOICE: &str = "lnbcrt1holdinvoicefromthescriptednode";

/// An order in the scripted node's book.
#[derive(Clone, Debug)]
pub struct Listed {
    pub id: Uuid,
    pub kind: OrderKind,
    pub status: Status,
    /// The maker's trade key: the one key that may not take the order.
    pub maker: PublicKey,
    pub amount: i64,
    pub fiat_amount: i64,
    /// A range order takes its fiat amount from the taker, inside these bounds.
    pub range: Option<(i64, i64)>,
}

impl Listed {
    pub fn fixed(id: Uuid, kind: OrderKind, maker: PublicKey) -> Self {
        Self {
            id,
            kind,
            status: Status::Pending,
            maker,
            amount: 15_000,
            fiat_amount: 15,
            range: None,
        }
    }

    pub fn ranged(mut self, min: i64, max: i64) -> Self {
        self.range = Some((min, max));
        self.fiat_amount = 0;
        self
    }

    /// The order as the node sends it, in `status`.
    pub fn small(&self, status: Status, fiat_amount: i64) -> SmallOrder {
        SmallOrder::new(
            Some(self.id),
            Some(self.kind),
            Some(status),
            self.amount,
            "USD".to_string(),
            self.range.map(|(min, _)| min),
            self.range.map(|(_, max)| max),
            fiat_amount,
            "bank transfer".to_string(),
            1,
            None,
            None,
            None,
            Some(1_792_108_800),
            Some(1_792_195_200),
        )
    }
}

/// What the scripted node remembers about takes.
#[derive(Clone, Debug, Default)]
pub struct Book {
    pub orders: Vec<Listed>,
    pub last_trade_index: i64,
    /// Identities that took an order and have one pending.
    pub takers: HashSet<PublicKey>,
}

/// The daemon's checks on a take, in the order of its source
/// (`check_trade_index`, then `take_sell_action` / `take_buy_action`), the
/// state it keeps (the index only after every check passed, the order's status)
/// and the replies it sends. Returns the script and a handle on the book.
pub fn take_daemon(book: Book, bonded: bool) -> (Respond, Arc<Mutex<Book>>) {
    let state = Arc::new(Mutex::new(book));
    let shared = Arc::clone(&state);
    let respond: Respond = Arc::new(move |request: &Received| {
        let refuse = |reason| vec![Outgoing::to_requester(refusal(request, reason))];
        let selling = match request.action {
            Action::TakeSell => true,
            Action::TakeBuy => false,
            _ => return refuse(CantDoReason::InvalidAction),
        };
        let index = request.trade_index.unwrap_or(0);
        let mut book = shared.lock().unwrap();
        if index <= book.last_trade_index || request.author == request.identity {
            return refuse(CantDoReason::InvalidTradeIndex);
        }
        if !request.signature_valid {
            return refuse(CantDoReason::InvalidSignature);
        }
        let Some(position) = book
            .orders
            .iter()
            .position(|order| Some(order.id) == request.order_id)
        else {
            return refuse(CantDoReason::NotFound);
        };
        if book.takers.contains(&request.identity) {
            return refuse(CantDoReason::PendingOrderExists);
        }
        let order = book.orders[position].clone();
        let wanted = if selling {
            OrderKind::Sell
        } else {
            OrderKind::Buy
        };
        if order.kind != wanted {
            return refuse(CantDoReason::InvalidOrderKind);
        }
        if order.status != Status::Pending {
            return refuse(CantDoReason::InvalidOrderStatus);
        }
        if order.maker == request.author {
            return refuse(CantDoReason::InvalidPubkey);
        }
        let fiat_amount = match order.range {
            Some((min, max)) => match request.amount {
                Some(amount) if (min..=max).contains(&amount) => amount,
                _ => return refuse(CantDoReason::OutOfRangeSatsAmount),
            },
            None => order.fiat_amount,
        };
        if request
            .invoice
            .as_deref()
            .is_some_and(|invoice| !invoice.starts_with("ln") && !invoice.contains('@'))
        {
            return refuse(CantDoReason::InvalidInvoice);
        }
        // The index is remembered only after every check passed.
        book.last_trade_index = index;
        book.takers.insert(request.identity);
        let taken = |status| order.small(status, fiat_amount);
        let message = |action, payload| {
            Message::new_order(
                Some(order.id),
                request.request_id,
                request.trade_index,
                action,
                payload,
            )
        };
        if bonded {
            book.orders[position].status = Status::WaitingTakerBond;
            let payload = Payload::PaymentRequest(
                Some(taken(Status::Pending)),
                "lnbcrt1bondinvoice".to_string(),
                None,
            );
            return vec![Outgoing::to_requester(message(
                Action::PayBondInvoice,
                Some(payload),
            ))];
        }
        match (selling, request.invoice.is_some()) {
            // The taker buys and has no invoice yet: the order, to add one to.
            (true, false) => {
                book.orders[position].status = Status::WaitingBuyerInvoice;
                vec![Outgoing::to_requester(message(
                    Action::AddInvoice,
                    Some(Payload::Order(taken(Status::WaitingBuyerInvoice))),
                ))]
            }
            // The taker buys with an invoice: the hold invoice goes to the
            // maker and the taker only hears that the seller must pay.
            (true, true) => {
                book.orders[position].status = Status::WaitingPayment;
                let pay = Payload::PaymentRequest(
                    Some(taken(Status::WaitingPayment)),
                    HOLD_INVOICE.to_string(),
                    None,
                );
                vec![
                    Outgoing::to_requester(message(Action::PayInvoice, Some(pay)))
                        .addressed_to(order.maker),
                    Outgoing::to_requester(message(Action::WaitingSellerToPay, None)),
                ]
            }
            // The taker sells: the hold invoice to pay.
            (false, _) => {
                book.orders[position].status = Status::WaitingPayment;
                let pay = Payload::PaymentRequest(
                    Some(taken(Status::WaitingPayment)),
                    HOLD_INVOICE.to_string(),
                    None,
                );
                vec![Outgoing::to_requester(message(
                    Action::PayInvoice,
                    Some(pay),
                ))]
            }
        }
    });
    (respond, state)
}

/// An order of the scripted node that waits for its buyer to answer.
#[derive(Clone, Debug)]
pub struct Owed {
    pub id: Uuid,
    /// The buyer's trade key: the one key that may add an invoice.
    pub buyer: PublicKey,
    pub status: Status,
    pub amount: i64,
    /// A stored preimage means the seller already paid: the order goes active.
    pub preimage: bool,
    /// The invoice the node stored for the buyer.
    pub invoice: Option<String>,
}

impl Owed {
    pub fn waiting(id: Uuid, buyer: PublicKey) -> Self {
        Self {
            id,
            buyer,
            status: Status::WaitingBuyerInvoice,
            amount: 15_000,
            preimage: false,
            invoice: None,
        }
    }

    pub fn small(&self, status: Status) -> SmallOrder {
        SmallOrder::new(
            Some(self.id),
            Some(OrderKind::Sell),
            Some(status),
            self.amount,
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
}

/// A slashed bond whose share the node owes to `recipient`.
#[derive(Clone, Debug)]
pub struct BondOwed {
    pub order: Uuid,
    pub recipient: PublicKey,
    pub share: i64,
    /// Still waiting for an invoice (`pending-payout`).
    pub open: bool,
    pub invoice: Option<String>,
}

/// What the scripted node remembers about invoices it waits for.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub orders: Vec<Owed>,
    pub bonds: Vec<BondOwed>,
}

/// The one invoice the scripted node accepts for a bond share of `share` sats.
pub fn payout_invoice(share: i64) -> String {
    format!("lnbcrt{share}n1bondpayoutinvoice")
}

/// The daemon's rules for `add-invoice` and `add-bond-invoice`, from its source
/// (`add_invoice_action`, `add_bond_invoice_action`): neither carries a trade
/// index; the order is found by id; only the buyer (or the bond's recipient)
/// may answer; the invoice is validated; then the status decides the reply.
/// `acks` models a node (v0.20.1) that sends `bond-invoice-accepted`, which
/// carries **no request id**; without it the node stays silent on success,
/// as older nodes do.
pub fn invoice_daemon(ledger: Ledger, acks: bool) -> (Respond, Arc<Mutex<Ledger>>) {
    let state = Arc::new(Mutex::new(ledger));
    let shared = Arc::clone(&state);
    let respond: Respond = Arc::new(move |request: &Received| {
        let refuse = |reason| vec![Outgoing::to_requester(refusal(request, reason))];
        let mut ledger = shared.lock().unwrap();
        match request.action {
            Action::AddInvoice => {
                let Some(position) = ledger
                    .orders
                    .iter()
                    .position(|order| Some(order.id) == request.order_id)
                else {
                    return refuse(CantDoReason::NotFound);
                };
                let order = ledger.orders[position].clone();
                if order.buyer != request.author {
                    return refuse(CantDoReason::InvalidPeer);
                }
                let Some(invoice) = request.invoice.clone() else {
                    return refuse(CantDoReason::InvalidInvoice);
                };
                if !invoice.starts_with("ln") && !invoice.contains('@') {
                    return refuse(CantDoReason::InvalidInvoice);
                }
                let reply = |action, payload| {
                    Message::new_order(Some(order.id), request.request_id, None, action, payload)
                };
                let entry = &mut ledger.orders[position];
                match (order.status, order.preimage) {
                    (Status::WaitingBuyerInvoice, true) => {
                        entry.status = Status::Active;
                        entry.invoice = Some(invoice);
                        vec![Outgoing::to_requester(reply(
                            Action::HoldInvoicePaymentAccepted,
                            Some(Payload::Order(order.small(Status::Active))),
                        ))]
                    }
                    (Status::WaitingBuyerInvoice, false) => {
                        entry.status = Status::WaitingPayment;
                        entry.invoice = Some(invoice);
                        vec![Outgoing::to_requester(reply(
                            Action::WaitingSellerToPay,
                            None,
                        ))]
                    }
                    (Status::SettledHoldInvoice, _) => {
                        entry.invoice = Some(invoice);
                        vec![Outgoing::to_requester(reply(Action::InvoiceUpdated, None))]
                    }
                    _ => refuse(CantDoReason::NotAllowedByStatus),
                }
            }
            Action::AddBondInvoice => {
                let Some(position) = ledger.bonds.iter().position(|bond| {
                    Some(bond.order) == request.order_id
                        && bond.recipient == request.author
                        && bond.open
                }) else {
                    return refuse(CantDoReason::NotAllowedByStatus);
                };
                let bond = ledger.bonds[position].clone();
                match request.invoice.as_deref() {
                    Some(invoice) if invoice == payout_invoice(bond.share) => {}
                    _ => return refuse(CantDoReason::InvalidInvoice),
                }
                ledger.bonds[position].open = false;
                ledger.bonds[position].invoice = request.invoice.clone();
                if !acks {
                    return Vec::new();
                }
                let small = SmallOrder::new(
                    Some(bond.order),
                    Some(OrderKind::Sell),
                    Some(Status::Success),
                    bond.share,
                    "USD".to_string(),
                    None,
                    None,
                    0,
                    "bank transfer".to_string(),
                    0,
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                vec![Outgoing::to_requester(Message::new_order(
                    Some(bond.order),
                    None,
                    None,
                    Action::BondInvoiceAccepted,
                    Some(Payload::Order(small)),
                ))]
            }
            _ => refuse(CantDoReason::InvalidAction),
        }
    });
    (respond, state)
}

// --- Trade steps: fiat-sent, release, cancel ---------------------------------

/// One trade of the scripted node, as far as the three steps need it.
#[derive(Clone, Debug)]
pub struct Traded {
    pub id: Uuid,
    pub status: Status,
    /// The maker's trade key.
    pub maker: PublicKey,
    pub buyer: Option<PublicKey>,
    pub seller: Option<PublicKey>,
    /// Who started a cooperative cancel.
    pub initiator: Option<PublicKey>,
    /// The key and index the node kept from a range `fiat-sent` or `release`.
    pub next_trade: Option<(String, u32)>,
}

impl Traded {
    /// An active trade: the maker sells to `buyer`.
    pub fn active(id: Uuid, seller: PublicKey, buyer: PublicKey) -> Self {
        Self {
            id,
            status: Status::Active,
            maker: seller,
            buyer: Some(buyer),
            seller: Some(seller),
            initiator: None,
            next_trade: None,
        }
    }

    /// A pending order of `maker`, with no taker.
    pub fn pending(id: Uuid, maker: PublicKey) -> Self {
        Self {
            id,
            status: Status::Pending,
            maker,
            buyer: None,
            seller: None,
            initiator: None,
            next_trade: None,
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    fn taker(&self) -> Option<PublicKey> {
        [self.buyer, self.seller]
            .into_iter()
            .flatten()
            .find(|key| *key != self.maker)
    }

    pub fn small(&self) -> SmallOrder {
        SmallOrder::new(
            Some(self.id),
            Some(OrderKind::Sell),
            Some(self.status),
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
}

/// The daemon's rules for `fiat-sent`, `release` and `cancel`, from its source
/// (`fiat_sent_action`, `release_action`, `cancel_action_generic`). None of
/// them carries a trade index. Every reply to the sender echoes the request
/// id; the notice to the other party does not. The order is found by id.
pub fn step_daemon(trades: Vec<Traded>) -> (Respond, Arc<Mutex<Vec<Traded>>>) {
    let state = Arc::new(Mutex::new(trades));
    let shared = Arc::clone(&state);
    let respond: Respond = Arc::new(move |request: &Received| {
        let refuse = |reason| vec![Outgoing::to_requester(refusal(request, reason))];
        let mut trades = shared.lock().unwrap();
        let Some(trade) = trades
            .iter_mut()
            .find(|trade| Some(trade.id) == request.order_id)
        else {
            return refuse(CantDoReason::NotFound);
        };
        let id = trade.id;
        let to_sender = |action, payload| {
            Outgoing::to_requester(Message::new_order(
                Some(id),
                request.request_id,
                None,
                action,
                payload,
            ))
        };
        let notice = |to: PublicKey, action, payload| {
            Outgoing::to_requester(Message::new_order(Some(id), None, None, action, payload))
                .addressed_to(to)
        };
        let peer = |key: PublicKey| {
            Some(Payload::Peer(Peer {
                pubkey: key.to_string(),
                reputation: None,
            }))
        };
        match request.action {
            Action::FiatSent => {
                if trade.status != Status::Active {
                    return refuse(CantDoReason::NotAllowedByStatus);
                }
                let (Some(buyer), Some(seller)) = (trade.buyer, trade.seller) else {
                    return refuse(CantDoReason::NotAllowedByStatus);
                };
                if request.author != buyer {
                    return refuse(CantDoReason::InvalidPubkey);
                }
                trade.status = Status::FiatSent;
                trade.next_trade = request.next_trade.clone();
                vec![
                    notice(seller, Action::FiatSentOk, peer(buyer)),
                    to_sender(Action::FiatSentOk, peer(seller)),
                ]
            }
            Action::Release => {
                let (Some(buyer), Some(seller)) = (trade.buyer, trade.seller) else {
                    return refuse(CantDoReason::NotAllowedByStatus);
                };
                if request.author != seller {
                    return refuse(CantDoReason::InvalidPeer);
                }
                if !matches!(trade.status, Status::FiatSent | Status::Dispute) {
                    return refuse(CantDoReason::NotAllowedByStatus);
                }
                trade.status = Status::SettledHoldInvoice;
                trade.next_trade = request.next_trade.clone();
                vec![
                    notice(buyer, Action::Released, None),
                    to_sender(Action::HoldInvoicePaymentSettled, None),
                    notice(seller, Action::Rate, None),
                ]
            }
            Action::Cancel => cancel_reply(trade, request, to_sender, notice),
            _ => refuse(CantDoReason::InvalidAction),
        }
    });
    (respond, state)
}

fn cancel_reply(
    trade: &mut Traded,
    request: &Received,
    to_sender: impl Fn(Action, Option<Payload>) -> Outgoing,
    notice: impl Fn(PublicKey, Action, Option<Payload>) -> Outgoing,
) -> Vec<Outgoing> {
    let refuse = |reason| vec![Outgoing::to_requester(refusal(request, reason))];
    if matches!(
        trade.status,
        Status::Canceled | Status::CooperativelyCanceled | Status::CanceledByAdmin
    ) {
        return refuse(CantDoReason::OrderAlreadyCanceled);
    }
    let sender = request.author;
    let other = [trade.buyer, trade.seller]
        .into_iter()
        .flatten()
        .find(|key| *key != sender);
    match trade.status {
        Status::Pending | Status::WaitingTakerBond => {
            if sender == trade.maker || Some(sender) == trade.taker() {
                trade.status = Status::Canceled;
                vec![to_sender(Action::Canceled, None)]
            } else {
                refuse(CantDoReason::IsNotYourOrder)
            }
        }
        Status::WaitingPayment | Status::WaitingBuyerInvoice => {
            if Some(sender) != trade.buyer && Some(sender) != trade.seller {
                return refuse(CantDoReason::IsNotYourOrder);
            }
            trade.status = Status::Canceled;
            let mut out = vec![to_sender(Action::Canceled, None)];
            out.extend(other.map(|key| notice(key, Action::Canceled, None)));
            out
        }
        Status::Active | Status::FiatSent | Status::Dispute => {
            let (Some(other), true) = (
                other,
                Some(sender) == trade.buyer || Some(sender) == trade.seller,
            ) else {
                return refuse(CantDoReason::InvalidPubkey);
            };
            match trade.initiator {
                None => {
                    trade.initiator = Some(sender);
                    vec![
                        to_sender(Action::CooperativeCancelInitiatedByYou, None),
                        notice(other, Action::CooperativeCancelInitiatedByPeer, None),
                    ]
                }
                Some(initiator) if initiator == other => {
                    trade.status = Status::CooperativelyCanceled;
                    vec![
                        to_sender(Action::CooperativeCancelAccepted, None),
                        notice(other, Action::CooperativeCancelAccepted, None),
                    ]
                }
                Some(_) => refuse(CantDoReason::InvalidPubkey),
            }
        }
        _ => refuse(CantDoReason::NotAllowedByStatus),
    }
}

// --- The mailbox: messages the node volunteers --------------------------------

/// A message as the relay would hold it after the node sent it to `to`: authored
/// and signed by `node`, encrypted for `to`, tagged with it, created at `at`.
/// The scripted node answers requests; this is for what a real node sends on its
/// own (after a bond is paid, when a seller pays, when a bond is slashed).
pub async fn node_event(node: &Keys, to: PublicKey, message: &Message, at: Timestamp) -> Event {
    let wrapped = wrap_message_with(
        Transport::Nip44Direct,
        message,
        node,
        node,
        to,
        WrapOptions::default(),
    )
    .await
    .unwrap();
    EventBuilder::new(Kind::PrivateDirectMessage, wrapped.content.clone())
        .tags(wrapped.tags.clone())
        .custom_created_at(at)
        .finalize(node)
        .unwrap()
}

/// `pay-bond-invoice`: the parked order and the bond invoice.
pub fn mail_pay_bond_invoice(order: &SmallOrder, invoice: &str) -> Message {
    Message::new_order(
        order.id,
        Some(1),
        None,
        Action::PayBondInvoice,
        Some(Payload::PaymentRequest(
            Some(order.clone()),
            invoice.to_string(),
            None,
        )),
    )
}

/// `add-invoice`: the order to answer with an invoice.
pub fn mail_add_invoice(order: &SmallOrder) -> Message {
    Message::new_order(
        order.id,
        Some(1),
        None,
        Action::AddInvoice,
        Some(Payload::Order(order.clone())),
    )
}

/// `pay-invoice`: the hold invoice the seller must pay.
pub fn mail_pay_invoice(order: &SmallOrder, invoice: &str) -> Message {
    Message::new_order(
        order.id,
        Some(1),
        None,
        Action::PayInvoice,
        Some(Payload::PaymentRequest(
            Some(order.clone()),
            invoice.to_string(),
            None,
        )),
    )
}

/// `waiting-seller-to-pay`: no payload.
pub fn mail_waiting_seller(order: Uuid) -> Message {
    Message::new_order(Some(order), Some(1), None, Action::WaitingSellerToPay, None)
}

/// `cant-do` with a reason.
pub fn mail_cant_do(reason: CantDoReason) -> Message {
    Message::cant_do(None, Some(1), Some(Payload::CantDo(Some(reason))))
}

/// `add-bond-invoice`: the request for the payout invoice of a slashed bond.
pub fn mail_bond_payout_request(order: &SmallOrder, slashed_at: i64) -> Message {
    Message::new_order(
        order.id,
        None,
        None,
        Action::AddBondInvoice,
        Some(Payload::BondPayoutRequest(
            mostro_core::prelude::BondPayoutRequest {
                order: order.clone(),
                slashed_at,
            },
        )),
    )
}

/// A message with a free-text payload.
pub fn mail_text(order: Uuid, text: &str) -> Message {
    Message::new_order(
        Some(order),
        None,
        None,
        Action::HoldInvoicePaymentAccepted,
        Some(Payload::TextMessage(text.to_string())),
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
                    let signature_valid = match (request.message.as_json(), request.signature) {
                        (Ok(json), Some(signature)) => {
                            Message::verify_signature(json, request.sender, signature)
                        }
                        _ => false,
                    };
                    let order = match &inner.payload {
                        Some(Payload::Order(order)) => Some(order.clone()),
                        _ => None,
                    };
                    let (amount, invoice) = match &inner.payload {
                        Some(Payload::Amount(amount)) => (Some(*amount), None),
                        Some(Payload::PaymentRequest(_, invoice, amount)) => {
                            (*amount, Some(invoice.clone()))
                        }
                        _ => (None, None),
                    };
                    let seen = Received {
                        event_id: event.id,
                        author: request.sender,
                        identity: request.identity,
                        request_id: inner.request_id,
                        action: inner.action.clone(),
                        ids,
                        trade_index: inner.trade_index,
                        order,
                        signature_valid,
                        order_id: inner.id,
                        amount,
                        invoice,
                        next_trade: match &inner.payload {
                            Some(Payload::NextTrade(key, index)) => Some((key.clone(), *index)),
                            _ => None,
                        },
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

/// Record `index` as the identity's last trade index in the store at `dir`
/// (the seeded user row), as a saved order would.
pub async fn set_last_trade_index(dir: &Path, index: i64) {
    let options = SqliteConnectOptions::new().filename(dir.join("mcli.db"));
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("UPDATE users SET last_trade_index = ?")
        .bind(index)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
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
