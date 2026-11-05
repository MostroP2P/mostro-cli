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
            | Self::ReplyMalformed => RUNTIME_ERROR_EXIT,
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorInfo {
    code: ErrorCode,
    message: &'static str,
    /// The node's refusal reason, only on `node_refused`. Omitted otherwise, and
    /// left out of the derived schema so the pilot documents do not change; the
    /// commands that can emit it add the property themselves.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    reason: Option<String>,
}

impl ErrorInfo {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message(),
            reason: None,
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// This error with the node's refusal `reason`, kept only when it is a
    /// snake-case word; anything else is dropped, never echoed.
    pub(super) fn with_reason(mut self, reason: &str) -> Self {
        self.reason = is_reason(reason).then(|| reason.to_string());
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
