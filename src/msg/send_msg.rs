//! Wrapping and publishing one request, and publishing one chat message.
//! Private to `msg`: the wrapper only ever sees the result of an exchange or
//! of a chat send.

use std::convert::Infallible;

use futures::future::{pending, BoxFuture, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};
use mostro_core::prelude::{wrap_message_with, Transport, WrapOptions};
use nostr_sdk::prelude::{Event, MachineReadablePrefix, Relay};
use tokio::time::{timeout_at, Instant};

use super::exchange::Outbound;

/// The request as a protocol-v2 event: sealed by the identity, authored and
/// signed by the signer (the same key for the account-scoped form), mined
/// with `pow` bits of proof of work.
pub(super) async fn wrap(outbound: &Outbound<'_>, pow: u8) -> Option<Event> {
    let options = WrapOptions {
        pow,
        expiration: None,
        signed: true,
    };
    wrap_message_with(
        Transport::Nip44Direct,
        &outbound.message,
        outbound.identity,
        outbound.signer,
        outbound.recipient,
        options,
    )
    .await
    .ok()
}

/// Why no relay confirmed the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unconfirmed {
    /// Every relay answered with a machine-readable rejection, so none has it.
    Rejected,
    /// At least one relay failed without a clear answer (timeout, dropped
    /// connection); it may still have received the event.
    Ambiguous,
}

/// Classify failure texts: a rejection only if every one starts with a
/// machine-readable prefix such as `blocked:` or `pow:`.
pub(super) fn classify<'a>(failures: impl IntoIterator<Item = &'a str>) -> Unconfirmed {
    let mut seen = false;
    for failure in failures {
        seen = true;
        if MachineReadablePrefix::parse(failure).is_none() {
            return Unconfirmed::Ambiguous;
        }
    }
    if seen {
        Unconfirmed::Rejected
    } else {
        Unconfirmed::Ambiguous
    }
}

/// The request on its way to every relay.
pub(super) struct Publishing<'a> {
    sends: FuturesUnordered<BoxFuture<'a, Result<(), String>>>,
    failures: Vec<String>,
}

/// Send `event` once to each of `relays` (a relay that asks for
/// authentication first is sent it again once authenticated, as the SDK
/// does). Nothing is sent until the result is polled.
pub(super) fn publish<'a>(relays: &'a [Relay], event: &'a Event) -> Publishing<'a> {
    let sends = relays
        .iter()
        .map(|relay| {
            async move {
                relay
                    .send_event(event)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            .boxed()
        })
        .collect();
    Publishing {
        sends,
        failures: Vec::new(),
    }
}

impl Publishing<'_> {
    /// `Ok` as soon as one relay accepts. Otherwise the verdict once every
    /// relay has answered, or ambiguous if `deadline` passes first.
    pub(super) async fn confirmed(&mut self, deadline: Instant) -> Result<(), Unconfirmed> {
        loop {
            match timeout_at(deadline, self.sends.next()).await {
                Ok(Some(Ok(()))) => return Ok(()),
                Ok(Some(Err(failure))) => self.failures.push(failure),
                Ok(None) => return Err(classify(self.failures.iter().map(String::as_str))),
                Err(_) => return Err(Unconfirmed::Ambiguous),
            }
        }
    }

    /// Drive the relays that have not answered yet until each has, or until
    /// `deadline`: after one relay confirmed, the others still get the event
    /// (a relay may be authenticating first) before the connection closes.
    pub(super) async fn settle(&mut self, deadline: Instant) {
        let _ = timeout_at(deadline, async {
            while self.sends.next().await.is_some() {}
        })
        .await;
    }

    /// Drive the relays that have not answered yet, then wait forever. Polled
    /// beside the reply wait, so a relay that accepts late, or is still
    /// authenticating, is not cut off by an earlier acceptance.
    pub(super) async fn drive(&mut self) -> Infallible {
        while self.sends.next().await.is_some() {}
        pending().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_prefixed_rejections_from_every_relay_count_as_rejected() {
        assert_eq!(classify(["blocked: not allowed"]), Unconfirmed::Rejected);
        assert_eq!(
            classify([
                "blocked: no",
                "pow: need 20 bits",
                "rate-limited: slow down"
            ]),
            Unconfirmed::Rejected
        );
        assert_eq!(classify(["blocked: no", "timeout"]), Unconfirmed::Ambiguous);
        assert_eq!(classify(["connection reset"]), Unconfirmed::Ambiguous);
        assert_eq!(classify(["an unprefixed refusal"]), Unconfirmed::Ambiguous);
        assert_eq!(classify([]), Unconfirmed::Ambiguous);
    }
}
