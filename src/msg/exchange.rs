//! One request and its reply: the correlated read of the user's own data.
//!
//! Everything happens on one private connection: read what the node
//! advertises, mine the request with the work it asks for, arm the wait,
//! publish the request once, wait for the answer, and clean up within a
//! bounded time. Nothing is retried and a request is never split or repeated.
//! Local effects are the caller's business; this module writes nothing.

use std::time::Duration;

use mostro_core::prelude::{Action, Message};
use nostr_sdk::prelude::{Event, Filter, Keys, Kind, PublicKey, Relay, SubscriptionId};
use tokio::time::{timeout, Instant};
use uuid::Uuid;

use super::node_info::{probe, work_to_mine, Lane, NodeInfo};
use super::reply::{arm, Wait, Waited};
use super::send_msg::{publish, wrap, Unconfirmed};
use super::{PublicReadConfig, ReadLimits, Session};
use crate::outcome::{
    DaemonAcceptance, ErrorCode, PowShortfall, Publication, WarningCode, Warnings,
};
use crate::util::reply::ExpectedReply;

/// Time budgets of one exchange. Tests shrink them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExchangeLimits {
    /// Connecting every configured relay.
    pub connect: Duration,
    /// Reading the node's advertised information, best effort.
    pub probe: Duration,
    /// From publishing the request to the reply, including the wait for the
    /// relays' acknowledgements.
    pub reply: Duration,
    /// Unsubscribing and disconnecting.
    pub cleanup: Duration,
    /// Per-relay notification buffer (the SDK drops lagged notifications silently).
    pub notification_capacity: usize,
}

impl Default for ExchangeLimits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            probe: Duration::from_secs(3),
            reply: Duration::from_secs(15),
            cleanup: Duration::from_secs(1),
            notification_capacity: 2_048,
        }
    }
}

/// The request to publish.
pub struct Outbound<'a> {
    /// Binds the request to the account: the seal signer. The same key as
    /// `signer` for the account-scoped form (`order info`).
    pub identity: &'a Keys,
    /// Authors and signs the request and receives the reply: the trade key of
    /// an order, or the identity key for the account-scoped form.
    pub signer: &'a Keys,
    pub recipient: PublicKey,
    /// Must carry a request identifier: the reply is matched on it.
    pub message: Message,
    /// The least proof of work mined on the request event (`--pow` / `POW`):
    /// raised to what the node advertises for `lane`, up to
    /// [`AUTO_POW_CEILING`](super::AUTO_POW_CEILING).
    pub pow: u8,
    /// The node's proof-of-work lane for `signer`.
    pub lane: Lane,
    /// Refuse a node that advertises an unsupported protocol version. Off when
    /// the user chose the transport explicitly.
    pub check_protocol: bool,
}

/// The replies that may answer the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub allowed_actions: Vec<Action>,
    /// Acknowledgements the node sends without a request id, matched on the
    /// order they name: `(order, actions)`.
    pub by_order: Option<(Uuid, Vec<Action>)>,
}

/// The answer, and anything noteworthy that happened around it.
#[derive(Debug)]
pub struct Exchanged {
    reply: Message,
    warnings: Warnings,
}

impl Exchanged {
    pub fn reply(&self) -> &Message {
        &self.reply
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Why no reply was obtained, and what is known about the request.
#[derive(Debug)]
pub struct ExchangeFailure {
    code: ErrorCode,
    publication: Publication,
    daemon_acceptance: DaemonAcceptance,
    warnings: Warnings,
    work: Option<PowShortfall>,
}

impl ExchangeFailure {
    fn unsent(code: ErrorCode) -> Self {
        Self::new(code, Publication::NotAttempted)
    }

    fn published(code: ErrorCode) -> Self {
        Self::new(code, Publication::Published)
    }

    /// `pow_required`, with what the node asks and what was configured.
    fn short(work: PowShortfall, publication: Publication) -> Self {
        Self {
            work: Some(work),
            ..Self::new(ErrorCode::PowRequired, publication)
        }
    }

    fn new(code: ErrorCode, publication: Publication) -> Self {
        Self {
            code,
            publication,
            daemon_acceptance: DaemonAcceptance::Unknown,
            warnings: Warnings::new(),
            work: None,
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The work numbers of a `pow_required` failure, for the outcome.
    pub fn work(&self) -> Option<PowShortfall> {
        self.work
    }

    pub fn publication(&self) -> Publication {
        self.publication
    }

    pub fn daemon_acceptance(&self) -> DaemonAcceptance {
        self.daemon_acceptance
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Publish `outbound` once and return the reply that answers it.
pub async fn exchange(
    config: &PublicReadConfig,
    outbound: Outbound<'_>,
    expect: &Expect,
    limits: &ExchangeLimits,
) -> Result<Exchanged, ExchangeFailure> {
    let request_id = outbound
        .message
        .get_inner_message_kind()
        .request_id
        .ok_or_else(|| ExchangeFailure::unsent(ErrorCode::InternalError))?;
    let session = Session::connect(config, &read_limits(limits))
        .await
        .map_err(ExchangeFailure::unsent)?;

    let subscription = SubscriptionId::generate();
    let mut expected = ExpectedReply::new(request_id, expect.allowed_actions.clone());
    if let Some((order, actions)) = &expect.by_order {
        expected = expected.or_by_order(*order, actions.clone());
    }
    let result = run(&session, config, &outbound, &subscription, expected, limits).await;

    let mut warnings = Warnings::new();
    let cleanup = async {
        let unsubscribed = session.client.unsubscribe(&subscription).await.is_ok();
        session.client.shutdown().await;
        unsubscribed
    };
    if !matches!(timeout(limits.cleanup, cleanup).await, Ok(true)) {
        warnings.add(WarningCode::CleanupIncomplete);
    }
    match result {
        Ok(reply) => Ok(Exchanged { reply, warnings }),
        Err(mut failure) => {
            failure.warnings.merge(&warnings);
            Err(failure)
        }
    }
}

fn read_limits(limits: &ExchangeLimits) -> ReadLimits {
    ReadLimits {
        connect: limits.connect,
        notification_capacity: limits.notification_capacity,
        ..ReadLimits::default()
    }
}

/// Read what the node advertises, then build the request with the work it
/// asks for on the request's lane. Nothing is published on failure.
async fn prepare(
    session: &Session,
    config: &PublicReadConfig,
    outbound: &Outbound<'_>,
    limits: &ExchangeLimits,
) -> Result<(NodeInfo, Event, u8), ExchangeFailure> {
    let info = probe(&session.client, config.mostro_pubkey(), limits.probe).await;
    if outbound.check_protocol && info.protocol_unsupported() {
        return Err(ExchangeFailure::unsent(ErrorCode::ProtocolUnsupported));
    }
    let pow = work_to_mine(outbound.pow, info.required(outbound.lane))
        .map_err(|work| ExchangeFailure::short(work, Publication::NotAttempted))?;
    let event = wrap(outbound, pow)
        .await
        .ok_or_else(|| ExchangeFailure::unsent(ErrorCode::InternalError))?;
    Ok((info, event, pow))
}

async fn run(
    session: &Session,
    config: &PublicReadConfig,
    outbound: &Outbound<'_>,
    subscription: &SubscriptionId,
    expected: ExpectedReply,
    limits: &ExchangeLimits,
) -> Result<Message, ExchangeFailure> {
    let client = &session.client;
    let node = config.mostro_pubkey();
    let (info, event, mined) = prepare(session, config, outbound, limits).await?;

    // Arm the wait, then subscribe, and only then publish.
    let mut armed = arm(client, config.relays())
        .await
        .ok_or_else(|| ExchangeFailure::unsent(ErrorCode::InternalError))?;
    let filter = Filter::new()
        .kind(Kind::PrivateDirectMessage)
        .author(node)
        .pubkey(outbound.signer.public_key())
        .limit(0);
    let subscribed = client.subscribe(filter).with_id(subscription.clone()).await;
    if subscribed.map_or(true, |output| output.success.is_empty()) {
        return Err(ExchangeFailure::unsent(ErrorCode::RelayReadFailed));
    }

    // One deadline from publication to the reply: waiting for the relays'
    // acknowledgements spends the reply budget.
    let deadline = Instant::now() + limits.reply;
    let relays: Vec<Relay> = client.relays().await.into_values().collect();
    let mut publishing = publish(&relays, &event);
    let confirmed = match publishing.confirmed(deadline).await {
        Ok(()) => true,
        Err(Unconfirmed::Ambiguous) => false,
        Err(Unconfirmed::Rejected) => {
            return Err(ExchangeFailure::new(
                ErrorCode::RequestRejected,
                Publication::NotAttempted,
            ));
        }
    };

    let wait = Wait {
        identity: outbound.signer,
        node,
        sent: event.id,
        expected,
        subscription,
        deadline,
    };
    // The relays that have not answered keep being driven while waiting.
    let waited = tokio::select! {
        biased;
        waited = armed.wait(&wait) => waited,
        never = publishing.drive() => match never {},
    };
    match waited {
        // The answer proves the request was published, acknowledged or not.
        Waited::Reply(reply) => Ok(*reply),
        _ if !confirmed => Err(ExchangeFailure::new(
            ErrorCode::RequestRejected,
            Publication::Unknown,
        )),
        Waited::Lost => Err(ExchangeFailure::published(ErrorCode::RelayReadFailed)),
        Waited::Timeout => {
            // A node silently drops under-mined events, so an advertisement of
            // more work than was sent, on either lane, explains the silence.
            let late = probe(client, node, limits.probe).await;
            Err(match unmet_after_silence(&late, &info, mined) {
                Some(required) => ExchangeFailure::short(
                    PowShortfall::new(required, outbound.pow),
                    Publication::Published,
                ),
                None => ExchangeFailure::published(ErrorCode::ReplyTimeout),
            })
        }
    }
}

/// The highest work above `sent` that the node advertises, now or before.
fn unmet_after_silence(late: &NodeInfo, early: &NodeInfo, sent: u8) -> Option<u8> {
    late.unmet(sent).max(early.unmet(sent))
}
