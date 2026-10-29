//! What the node advertises in its kind-38385 info event, read once and on a
//! best-effort basis: its protocol version and the proof of work it expects.
//!
//! A missing, late or malformed event is not an error; the exchange then
//! proceeds with the defaults, as the legacy commands do.

use std::time::Duration;

use mostro_core::prelude::NOSTR_INFO_EVENT_KIND;
use nostr_sdk::prelude::{Client, Event, Filter, Kind, PublicKey};

use crate::util::events::read_info_tag_from_event;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct NodeInfo {
    pub protocol_version: Option<u8>,
    /// Work expected on any event.
    pub pow: Option<u8>,
    /// Work expected on a first contact from a key the node has not seen.
    pub pow_first_contact: Option<u8>,
}

impl NodeInfo {
    /// The newest info event among `events`, or nothing known.
    pub(super) fn from_events<'a>(events: impl IntoIterator<Item = &'a Event>) -> Self {
        events
            .into_iter()
            .max_by_key(|event| event.created_at)
            .map_or_else(Self::default, Self::from_event)
    }

    fn from_event(event: &Event) -> Self {
        let number = |name: &str| {
            read_info_tag_from_event(event, name).and_then(|value| value.trim().parse::<u8>().ok())
        };
        Self {
            protocol_version: number("protocol_version"),
            pow: number("pow"),
            pow_first_contact: number("pow_first_contact"),
        }
    }

    /// Protocol 1 (gift wrap) is no longer spoken. Version 2, another number or
    /// no advertisement all mean the NIP-44 transport.
    pub(super) fn protocol_unsupported(&self) -> bool {
        self.protocol_version == Some(1)
    }

    /// The node expects more work on any event than is configured, so it would
    /// drop the request silently.
    pub(super) fn work_short(&self, configured: u8) -> bool {
        self.pow.is_some_and(|required| required > configured)
    }

    /// The work expected on a first contact is above what is configured.
    pub(super) fn first_contact_work_short(&self, configured: u8) -> bool {
        self.pow_first_contact
            .is_some_and(|required| required > configured)
    }
}

/// Read the node's info event within `budget`; anything that goes wrong is
/// "nothing known".
pub(super) async fn probe(client: &Client, node: PublicKey, budget: Duration) -> NodeInfo {
    let filter = Filter::new()
        .author(node)
        .kind(Kind::Custom(NOSTR_INFO_EVENT_KIND));
    match client.fetch_events(filter).timeout(budget).await {
        Ok(events) => NodeInfo::from_events(events.iter()),
        Err(_) => NodeInfo::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::{EventBuilder, FinalizeEvent, Keys, Tag, Timestamp};

    fn info(at: u64, tags: &[(&str, &str)]) -> Event {
        EventBuilder::new(Kind::Custom(NOSTR_INFO_EVENT_KIND), "")
            .tags(
                tags.iter()
                    .map(|(name, value)| Tag::custom(*name, [*value])),
            )
            .custom_created_at(Timestamp::from(at))
            .finalize(&Keys::generate())
            .unwrap()
    }

    #[test]
    fn the_newest_event_wins_and_tags_are_read_as_numbers() {
        let old = info(10, &[("protocol_version", "1"), ("pow", "20")]);
        let new = info(
            20,
            &[
                ("protocol_version", " 2 "),
                ("pow", "8"),
                ("pow_first_contact", "12"),
            ],
        );
        let info = NodeInfo::from_events([&old, &new]);
        assert_eq!(
            info,
            NodeInfo {
                protocol_version: Some(2),
                pow: Some(8),
                pow_first_contact: Some(12)
            }
        );
    }

    #[test]
    fn nothing_known_is_not_an_error() {
        assert_eq!(NodeInfo::from_events([]), NodeInfo::default());
        let broken = info(
            1,
            &[
                ("protocol_version", "two"),
                ("pow", "999"),
                ("pow_first_contact", ""),
            ],
        );
        assert_eq!(NodeInfo::from_events([&broken]), NodeInfo::default());
        let nothing = NodeInfo::default();
        assert!(!nothing.protocol_unsupported());
        assert!(!nothing.work_short(0) && !nothing.first_contact_work_short(0));
    }

    #[test]
    fn only_protocol_one_is_unsupported() {
        for (version, unsupported) in [
            (Some(1), true),
            (Some(2), false),
            (Some(3), false),
            (None, false),
        ] {
            let info = NodeInfo {
                protocol_version: version,
                ..NodeInfo::default()
            };
            assert_eq!(info.protocol_unsupported(), unsupported, "{version:?}");
        }
    }

    #[test]
    fn work_is_short_only_when_the_node_expects_more_than_configured() {
        let info = NodeInfo {
            pow: Some(8),
            pow_first_contact: Some(12),
            ..NodeInfo::default()
        };
        assert!(info.work_short(7) && !info.work_short(8) && !info.work_short(9));
        assert!(info.first_contact_work_short(11) && !info.first_contact_work_short(12));
    }
}
