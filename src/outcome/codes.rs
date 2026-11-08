//! Error and warning codes of the outcome envelope: their wire names, fixed
//! prose and exit statuses. The pilot sets are frozen; later commands add codes
//! that appear only in their own generated schemas.

use schemars::JsonSchema;
use serde::Serialize;

use super::{ARGUMENT_ERROR_EXIT, RUNTIME_ERROR_EXIT};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArguments,
    OutputNotSupported,
    ConfigurationMissing,
    ConfigurationInvalid,
    RelayConnectionFailed,
    RelayReadFailed,
    ReadTimeout,
    ReadLimitExceeded,
    InternalError,
    // Added for `order.info`. They appear only in the schemas of commands that
    // can emit them; the pilot documents keep the first nine.
    IdentityMissing,
    IdentityUnreadable,
    ProtocolUnsupported,
    PowRequired,
    RequestRejected,
    ReplyTimeout,
    NodeRefused,
    ReplyMalformed,
    // Added for `order.new`; scoped to that command's schema.
    OrderDeclined,
    OrderNotSaved,
    StoreUnwritable,
    // Added for `order.add-invoice` and `bond.add-invoice`.
    OrderUnknown,
    // Added for `message.list`.
    StoreUnreadable,
    // Added for `identity.init`.
    IdentityExists,
    // Added for `order.chat`.
    CounterpartyUnknown,
}

impl ErrorCode {
    /// The codes of the order-list pilot, frozen: its generated schemas list
    /// exactly these.
    pub const PILOT: [ErrorCode; 9] = [
        Self::InvalidArguments,
        Self::OutputNotSupported,
        Self::ConfigurationMissing,
        Self::ConfigurationInvalid,
        Self::RelayConnectionFailed,
        Self::RelayReadFailed,
        Self::ReadTimeout,
        Self::ReadLimitExceeded,
        Self::InternalError,
    ];

    pub fn exit_code(self) -> i32 {
        match self {
            Self::InvalidArguments | Self::OutputNotSupported => ARGUMENT_ERROR_EXIT,
            Self::ConfigurationMissing
            | Self::ConfigurationInvalid
            | Self::RelayConnectionFailed
            | Self::RelayReadFailed
            | Self::ReadTimeout
            | Self::ReadLimitExceeded
            | Self::InternalError
            | Self::IdentityMissing
            | Self::IdentityUnreadable
            | Self::ProtocolUnsupported
            | Self::PowRequired
            | Self::RequestRejected
            | Self::ReplyTimeout
            | Self::NodeRefused
            | Self::ReplyMalformed
            | Self::OrderDeclined
            | Self::OrderNotSaved
            | Self::StoreUnwritable
            | Self::OrderUnknown
            | Self::StoreUnreadable
            | Self::IdentityExists
            | Self::CounterpartyUnknown => RUNTIME_ERROR_EXIT,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidArguments => "The command arguments are invalid.",
            Self::OutputNotSupported => "This command does not support JSON output.",
            Self::ConfigurationMissing => "Required node configuration is missing.",
            Self::ConfigurationInvalid => "The node configuration is invalid.",
            Self::RelayConnectionFailed => "A configured relay could not be connected.",
            Self::RelayReadFailed => "A configured relay failed during the public order read.",
            Self::ReadTimeout => "The public order read did not complete before its deadline.",
            Self::ReadLimitExceeded => "The public order read exceeded its event limit.",
            Self::InternalError => "An internal error occurred.",
            Self::IdentityMissing => "No local identity exists.",
            Self::IdentityUnreadable => "The local identity store cannot be read.",
            Self::ProtocolUnsupported => {
                "The node speaks a protocol version this program does not support."
            }
            Self::PowRequired => "The node requires more proof of work than is configured.",
            Self::RequestRejected => "No relay confirmed accepting the request.",
            Self::ReplyTimeout => "The node did not answer before the deadline.",
            Self::NodeRefused => "The node refused the request.",
            Self::ReplyMalformed => "The reply from the node could not be used.",
            Self::OrderDeclined => "The order was not confirmed, so nothing was sent.",
            Self::OrderNotSaved => "The node created the order but it could not be saved locally.",
            Self::StoreUnwritable => "The local store cannot be written, so nothing was sent.",
            Self::OrderUnknown => "No local order has this id, so nothing was sent.",
            Self::StoreUnreadable => "The local store cannot be read.",
            Self::IdentityExists => "A local identity already exists, so nothing was changed.",
            Self::CounterpartyUnknown => {
                "The counterparty of this order was not found in the node's messages; \
                 it is known once the escrow is locked."
            }
        }
    }
}

/// The reason of an `invalid_arguments` caused by the invoice value itself
/// (the program refused it before anything was sent), as a node refusal
/// carries its own reason.
pub const INVALID_INVOICE: &str = "invalid_invoice";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorInfo {
    code: ErrorCode,
    message: &'static str,
    /// The node's refusal reason on `node_refused`, or [`INVALID_INVOICE`] on an
    /// `invalid_arguments` caused by the invoice. Omitted otherwise, and left
    /// out of the derived schema so the pilot documents do not change; the
    /// commands that can emit it add the property themselves.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    reason: Option<String>,
    /// On `pow_required` only: the proof of work the node advertises for the
    /// request, in bits. Left out of the derived schema like `reason`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    required: Option<u8>,
    /// On `pow_required` only: the `--pow` / `POW` floor, in bits.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    configured: Option<u8>,
}

/// What a `pow_required` error tells about the work: what the node advertises
/// for the request and the floor the user configured, both in bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowShortfall {
    required: u8,
    configured: u8,
}

impl PowShortfall {
    pub fn new(required: u8, configured: u8) -> Self {
        Self {
            required,
            configured,
        }
    }

    pub fn required(&self) -> u8 {
        self.required
    }

    pub fn configured(&self) -> u8 {
        self.configured
    }
}

impl ErrorInfo {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message(),
            reason: None,
            required: None,
            configured: None,
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// The work numbers of a `pow_required` error.
    pub fn work(&self) -> Option<PowShortfall> {
        self.required
            .zip(self.configured)
            .map(|(required, configured)| PowShortfall::new(required, configured))
    }

    /// This error with the node's refusal `reason`, kept only when it is a
    /// snake-case word; anything else is dropped, never echoed.
    pub(super) fn with_reason(mut self, reason: &str) -> Self {
        self.reason = is_reason(reason).then(|| reason.to_string());
        self
    }

    /// This error with the numbers of `work`, kept only on `pow_required`.
    pub(super) fn with_work(mut self, work: PowShortfall) -> Self {
        if self.code == ErrorCode::PowRequired {
            self.required = Some(work.required);
            self.configured = Some(work.configured);
        }
        self
    }
}

/// Whether `reason` is a snake-case word, the only form a refusal reason may
/// take on the wire.
fn is_reason(reason: &str) -> bool {
    let mut chars = reason.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && reason.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    InvalidEventTimestamp,
    CleanupIncomplete,
    // Added for `order.info`; scoped to that command's schema.
    OrdersNotReturned,
    InvalidOrderTimestamp,
    // Added for `order.add-invoice`; shared by every write that sets the local status.
    LocalStatusStale,
    // Added for `message.list`.
    TradeKeysLimited,
}

impl WarningCode {
    /// The warnings of the order-list pilot, frozen like [`ErrorCode::PILOT`].
    pub const PILOT: [WarningCode; 2] = [Self::InvalidEventTimestamp, Self::CleanupIncomplete];

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidEventTimestamp => {
                "One or more event timestamps cannot be shown as a UTC date."
            }
            Self::CleanupIncomplete => "Relay cleanup did not finish within its time budget.",
            Self::OrdersNotReturned => "Some requested orders were not returned by the node.",
            Self::InvalidOrderTimestamp => {
                "One or more order timestamps cannot be shown as a UTC date."
            }
            Self::TradeKeysLimited => "Trade keys older than the newest 200 were not read.",
            Self::LocalStatusStale => {
                "The node accepted the request but the local order status could not be updated."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Warning {
    code: WarningCode,
    message: &'static str,
}

/// Warnings of one outcome, at most one per code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Warnings(Vec<Warning>);

impl Warnings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `code`; repeated occurrences aggregate into one warning.
    pub fn add(&mut self, code: WarningCode) {
        if !self.contains(code) {
            self.0.push(Warning {
                code,
                message: code.message(),
            });
        }
    }

    pub fn contains(&self, code: WarningCode) -> bool {
        self.0.iter().any(|warning| warning.code == code)
    }

    /// Record every warning of `other`, still one per code.
    pub fn merge(&mut self, other: &Warnings) {
        for warning in &other.0 {
            self.add(warning.code);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
