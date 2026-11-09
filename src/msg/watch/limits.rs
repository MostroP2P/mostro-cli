//! The bounds of a watch and why one did not open.

use std::time::Duration;

use crate::msg::ReadLimits;
use crate::outcome::{ErrorCode, Warnings};

/// The fixed wait before a relay that went away is tried again.
const RETRY: Duration = Duration::from_secs(10);

/// Bounds of a watch. Tests shrink them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchLimits {
    /// As for a read: `connect` for every relay, `read` for each replay,
    /// `cleanup`, `max_unique_events` for the records of one run and
    /// `notification_capacity` for one replay.
    pub read: ReadLimits,
    /// The fixed wait before a relay that went away is tried again (10 s).
    /// At least 1 s: the SDK stamps subscriptions and connections in whole
    /// seconds and sends a subscription again only when the reconnect falls
    /// in a later second than its REQ.
    pub retry: Duration,
}

impl Default for WatchLimits {
    fn default() -> Self {
        Self {
            read: ReadLimits::default(),
            retry: RETRY,
        }
    }
}

/// Why a watch did not open.
#[derive(Clone, Debug)]
pub struct WatchFailure {
    pub(super) code: ErrorCode,
    pub(super) warnings: Warnings,
}

impl WatchFailure {
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// `cleanup_incomplete` when the connection was not closed in time.
    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}
