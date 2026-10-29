//! One request and its reply: the correlated read of the user's own data.
//!
//! Everything happens on one private connection: read what the node
//! advertises, arm the wait, publish the request once, wait for the answer, and
//! clean up within a bounded time. Nothing is retried and a request is never
//! split or repeated. Local effects are the caller's business; this module
//! writes nothing.

use std::time::Duration;

use mostro_core::prelude::{Action, Message};
use nostr_sdk::prelude::{Filter, Keys, Kind, PublicKey, SubscriptionId};
use tokio::time::{timeout, Instant};

use super::node_info::{probe, NodeInfo};
use super::reply::{arm, Wait, Waited};
use super::send_msg::{publish, wrap, Unconfirmed};
use super::{PublicReadConfig, ReadLimits, Session};
use crate::outcome::{DaemonAcceptance, ErrorCode, Publication, WarningCode, Warnings};
use crate::util::reply::ExpectedReply;

/// Time budgets of one exchange. Tests shrink them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExchangeLimits {
    /// Connecting every configured relay.
    pub connect: Duration,
    /// Reading the node's advertised information, best effort.
    pub probe: Duration,
    /// From publishing the request to the reply.
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
    /// Signs the request and receives the reply (the account-scoped identity key).
    pub signer: &'a Keys,
    pub recipient: PublicKey,
    /// Must carry a request identifier: the reply is matched on it.
    pub message: Message,
    /// Proof of work mined on the request event.
    pub pow: u8,
    /// Refuse a node that advertises an unsupported protocol version. Off when
    /// the user chose the transport explicitly.
    pub check_protocol: bool,
}

/// The replies that may answer the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub allowed_actions: Vec<Action>,
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
}

impl ExchangeFailure {
    fn unsent(code: ErrorCode) -> Self {
        Self::new(code, Publication::NotAttempted)
    }

    fn published(code: ErrorCode) -> Self {
        Self::new(code, Publication::Published)
    }

    fn new(code: ErrorCode, publication: Publication) -> Self {
        Self {
            code,
            publication,
            daemon_acceptance: DaemonAcceptance::Unknown,
            warnings: Warnings::new(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
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
    let internal = || ExchangeFailure::unsent(ErrorCode::InternalError);
    let request_id = outbound
        .message
        .get_inner_message_kind()
        .request_id
        .ok_or_else(internal)?;
    let event = wrap(&outbound).await.ok_or_else(internal)?;
    let session = Session::connect(config, &read_limits(limits))
        .await
        .map_err(ExchangeFailure::unsent)?;

    let subscription = SubscriptionId::generate();
    let expected = ExpectedReply::new(request_id, expect.allowed_actions.clone());
    let result = run(
        &session,
        config,
        &outbound,
        &event,
        &subscription,
        expected,
        limits,
    )
    .await;

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

async fn run(
    session: &Session,
    config: &PublicReadConfig,
    outbound: &Outbound<'_>,
    event: &nostr_sdk::prelude::Event,
    subscription: &SubscriptionId,
    expected: ExpectedReply,
    limits: &ExchangeLimits,
) -> Result<Message, ExchangeFailure> {
    let client = &session.client;
    let node = config.mostro_pubkey();

    let info = probe(client, node, limits.probe).await;
    if outbound.check_protocol && info.protocol_unsupported() {
        return Err(ExchangeFailure::unsent(ErrorCode::ProtocolUnsupported));
    }
    if info.work_short(outbound.pow) {
        return Err(ExchangeFailure::unsent(ErrorCode::PowRequired));
    }

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

    if let Err(unconfirmed) = publish(client, event).await {
        let publication = match unconfirmed {
            Unconfirmed::Rejected => Publication::NotAttempted,
            Unconfirmed::Ambiguous => Publication::Unknown,
        };
        return Err(ExchangeFailure::new(
            ErrorCode::RequestRejected,
            publication,
        ));
    }

    let wait = Wait {
        identity: outbound.signer,
        node,
        sent: event.id,
        expected,
        subscription,
        deadline: Instant::now() + limits.reply,
    };
    match armed.wait(&wait).await {
        Waited::Reply(reply) => Ok(*reply),
        Waited::Lost => Err(ExchangeFailure::published(ErrorCode::RelayReadFailed)),
        Waited::Timeout => {
            // A node silently drops under-mined events, so an advertisement of
            // more work than was sent explains the silence.
            let late = probe(client, node, limits.probe).await;
            let short = work_explains_silence(&late, &info, outbound.pow);
            Err(ExchangeFailure::published(if short {
                ErrorCode::PowRequired
            } else {
                ErrorCode::ReplyTimeout
            }))
        }
    }
}

/// Whether what the node advertises, now or before, is above the work sent.
fn work_explains_silence(late: &NodeInfo, early: &NodeInfo, sent: u8) -> bool {
    [late, early]
        .iter()
        .any(|info| info.work_short(sent) || info.first_contact_work_short(sent))
}
