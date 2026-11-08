//! What the node advertises in its kind-38385 info event, read once and on a
//! best-effort basis: its protocol version and the proof of work it expects,
//! and from that the work a request is mined with.
//!
//! A missing, late or malformed event is not an error; the exchange then
//! proceeds with the defaults, as the legacy commands do.

use std::time::Duration;

use mostro_core::prelude::NOSTR_INFO_EVENT_KIND;
use nostr_sdk::prelude::{Client, Event, Filter, Kind, PublicKey};

use crate::outcome::PowShortfall;
use crate::util::events::read_info_tag_from_event;

/// The most work mined without the user asking for it: about a million
/// hashes, a second or two on a laptop (mean 1.6 s, worst of five 5.2 s, on a
/// Ryzen 7 4800H in a release build). A node that asks for more is refused
/// unless `--pow` / `POW` reaches its value.
pub const AUTO_POW_CEILING: u8 = 20;

/// Which of the node's two proof-of-work lanes a request travels on. The node
/// decides on the event author, so the lane follows the key that signs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// The trade key of an order the node holds open: only `pow` applies.
    Known,
    /// A key the node may not know (a fresh trade key, the identity key, the
    /// key of a finished order): `pow_first_contact` applies as well.
    FirstContact,
}

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

    /// The work the node expects on `lane`, when it advertises any. A first
    /// contact clears both checks, so it needs the higher of the two values,
    /// as mostrod advertises it.
    pub(super) fn required(&self, lane: Lane) -> Option<u8> {
        match lane {
            Lane::Known => self.pow,
            Lane::FirstContact => self.pow.max(self.pow_first_contact),
        }
    }

    /// The highest advertised work above `sent` on either lane: what explains
    /// the silence of a node that drops under-mined events.
    pub(super) fn unmet(&self, sent: u8) -> Option<u8> {
        self.required(Lane::FirstContact)
            .filter(|&required| required > sent)
    }
}

/// The work to mine: the `configured` floor, raised to what the node
/// `required`s up to [`AUTO_POW_CEILING`]. Above it, only a floor that already
/// reaches the requirement is mined; otherwise nothing should be sent.
pub(super) fn work_to_mine(configured: u8, required: Option<u8>) -> Result<u8, PowShortfall> {
    match required {
        Some(required) if required > configured && required > AUTO_POW_CEILING => {
            Err(PowShortfall::new(required, configured))
        }
        Some(required) => Ok(required.max(configured)),
        None => Ok(configured),
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
        assert_eq!(nothing.required(Lane::FirstContact), None);
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

    fn advertising(pow: Option<u8>, pow_first_contact: Option<u8>) -> NodeInfo {
        NodeInfo {
            pow,
            pow_first_contact,
            ..NodeInfo::default()
        }
    }

    #[test]
    fn the_known_lane_needs_pow_and_a_first_contact_the_higher_of_both() {
        for (pow, first, known, first_contact) in [
            (Some(8), Some(12), Some(8), Some(12)),
            // A node that advertises a first-contact value below its base one
            // still checks the base one on every event.
            (Some(8), Some(2), Some(8), Some(8)),
            (Some(6), None, Some(6), Some(6)),
            (None, Some(16), None, Some(16)),
            (None, None, None, None),
        ] {
            let info = advertising(pow, first);
            assert_eq!(info.required(Lane::Known), known, "{info:?}");
            assert_eq!(info.required(Lane::FirstContact), first_contact, "{info:?}");
        }
    }

    #[test]
    fn unmet_work_is_the_highest_advertised_difficulty_above_what_was_sent() {
        let info = advertising(Some(8), Some(12));
        assert_eq!(info.unmet(0), Some(12));
        assert_eq!(info.unmet(8), Some(12));
        assert_eq!(info.unmet(12), None);
        assert_eq!(advertising(Some(8), None).unmet(7), Some(8));
        assert_eq!(NodeInfo::default().unmet(0), None);
    }

    #[test]
    fn the_work_mined_is_the_configured_floor_raised_to_the_node_up_to_the_ceiling() {
        // Nothing advertised, or no more than configured: the floor as given.
        assert_eq!(work_to_mine(0, None), Ok(0));
        assert_eq!(work_to_mine(10, Some(6)), Ok(10));
        assert_eq!(work_to_mine(6, Some(6)), Ok(6));
        // Raised on its own, up to the ceiling.
        assert_eq!(work_to_mine(0, Some(6)), Ok(6));
        assert_eq!(
            work_to_mine(0, Some(AUTO_POW_CEILING)),
            Ok(AUTO_POW_CEILING)
        );
        // Above it, only when the user asked for that much.
        let above = AUTO_POW_CEILING + 1;
        assert_eq!(
            work_to_mine(3, Some(above)),
            Err(PowShortfall::new(above, 3))
        );
        assert_eq!(work_to_mine(above, Some(above)), Ok(above));
        assert_eq!(work_to_mine(255, Some(above)), Ok(255));
    }
}
