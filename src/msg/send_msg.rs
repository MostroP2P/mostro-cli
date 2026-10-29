//! Wrapping and publishing one request. Private to `msg`: the wrapper only
//! ever sees the result of an exchange.

use mostro_core::prelude::{wrap_message_with, Transport, WrapOptions};
use nostr_sdk::prelude::{Client, Event, MachineReadablePrefix};

use super::exchange::Outbound;

/// The request as a protocol-v2 event: sealed by the identity, authored and
/// signed by the signer (the same key for the account-scoped form).
pub(super) async fn wrap(outbound: &Outbound<'_>) -> Option<Event> {
    let options = WrapOptions {
        pow: outbound.pow,
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

/// Publish `event` once. `Ok` when at least one relay accepted it.
pub(super) async fn publish(client: &Client, event: &Event) -> Result<(), Unconfirmed> {
    match client.send_event(event).await {
        Ok(output) if !output.success.is_empty() => Ok(()),
        Ok(output) => Err(classify(output.failed.values().map(String::as_str))),
        Err(_) => Err(Unconfirmed::Ambiguous),
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
