//! Machine-readable outcome envelope, contract version 1.
//!
//! An [`Envelope`] can only be built through its constructors, so status,
//! data and error always agree. Every key is serialized, as null when empty.
//! Error and warning messages are fixed prose chosen by their code; no
//! caller-provided text reaches the wire.

use schemars::JsonSchema;
use serde::Serialize;

pub use codes::{
    ErrorCode, ErrorInfo, PowShortfall, Warning, WarningCode, Warnings, INVALID_INVOICE,
};

pub mod admin_order_info;
pub mod chat;
mod codes;
pub mod config;
pub mod dispute;
pub mod identity;
pub mod info;
pub mod invoice;
pub mod messages;
pub mod mine;
pub mod new;
pub mod order;
pub mod rate;
mod rules;
pub mod schema;
pub mod step;
pub mod sync;
pub mod take;
pub mod watch;

/// Contract version, independent of the executable version.
pub const SCHEMA_VERSION: &str = "1";
/// Exit status for argument and unsupported-output errors.
pub const ARGUMENT_ERROR_EXIT: i32 = 2;
/// Exit status for configuration, read and internal errors.
pub const RUNTIME_ERROR_EXIT: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Error,
    /// The request may have reached the node, or the node accepted it and the
    /// local write failed. Never `ok`. Only commands that publish a write emit
    /// it, through [`Envelope::uncertain`].
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Publication {
    NotAttempted,
    Unknown,
    Published,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DaemonAcceptance {
    Unknown,
    Accepted,
    Rejected,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    NotAttempted,
    Applied,
    Failed,
    NotApplicable,
}

/// Execution stages and optional snapshot evidence of one outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ExecutionMeta<S> {
    publication: Publication,
    daemon_acceptance: DaemonAcceptance,
    persistence: Persistence,
    snapshot: Option<S>,
}

impl<S> ExecutionMeta<S> {
    /// A known read-only command: no write stage applies. `snapshot` is
    /// `None` until the public query has started.
    pub fn read(snapshot: Option<S>) -> Self {
        Self {
            publication: Publication::NotApplicable,
            daemon_acceptance: DaemonAcceptance::NotApplicable,
            persistence: Persistence::NotApplicable,
            snapshot,
        }
    }

    /// A read of the relays that may save rows locally (`order.sync`): nothing is
    /// published, `persistence` says what happened to the rows.
    pub fn sync(snapshot: Option<S>, persistence: Persistence) -> Self {
        Self {
            publication: Publication::NotApplicable,
            daemon_acceptance: DaemonAcceptance::NotApplicable,
            persistence,
            snapshot,
        }
    }

    /// A command that writes only local state and talks to nobody
    /// (`identity.init`): `persistence` says what happened to the write.
    pub fn local(persistence: Persistence) -> Self {
        Self::sync(None, persistence)
    }

    /// A command that publishes one request and writes nothing locally: the
    /// given publication and node-acceptance stages, persistence not applicable.
    pub fn request(publication: Publication, daemon_acceptance: DaemonAcceptance) -> Self {
        Self {
            publication,
            daemon_acceptance,
            persistence: Persistence::NotApplicable,
            snapshot: None,
        }
    }

    /// A command that publishes one request and writes locally: `persistence`
    /// says whether the write the outcome calls for was applied or failed.
    pub fn write(
        publication: Publication,
        daemon_acceptance: DaemonAcceptance,
        persistence: Persistence,
    ) -> Self {
        Self {
            publication,
            daemon_acceptance,
            persistence,
            snapshot: None,
        }
    }

    /// A legacy command rejected before execution, or an invocation whose
    /// command could not be resolved.
    pub fn not_executed() -> Self {
        Self {
            publication: Publication::NotAttempted,
            daemon_acceptance: DaemonAcceptance::Unknown,
            persistence: Persistence::NotAttempted,
            snapshot: None,
        }
    }
}

/// One controlled outcome. `D` is the success data and `S` the snapshot type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Envelope<D, S> {
    schema_version: &'static str,
    command: Option<&'static str>,
    status: Status,
    data: Option<D>,
    error: Option<ErrorInfo>,
    warnings: Warnings,
    meta: ExecutionMeta<S>,
}

impl<D, S> Envelope<D, S> {
    /// A success of the resolved canonical `command`.
    pub fn ok(command: &'static str, data: D, warnings: Warnings, meta: ExecutionMeta<S>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: Some(command),
            status: Status::Ok,
            data: Some(data),
            error: None,
            warnings,
            meta,
        }
    }

    /// A failure; `command` is `None` when it could not be resolved. Data is
    /// always null, even when some results were collected.
    pub fn error(
        command: Option<&'static str>,
        code: ErrorCode,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command,
            status: Status::Error,
            data: None,
            error: Some(ErrorInfo::new(code)),
            warnings,
            meta,
        }
    }

    /// The request may have reached the node (or the node accepted it and the
    /// local write failed): never `ok`, never retried by the program. `data`
    /// is what the node answered, when it did.
    pub fn uncertain(
        command: &'static str,
        code: ErrorCode,
        data: Option<D>,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: Some(command),
            status: Status::Uncertain,
            data,
            error: Some(ErrorInfo::new(code)),
            warnings,
            meta,
        }
    }

    /// A refusal by the node: `node_refused` with its reason when that is a
    /// snake-case word. Data is null, as for every error.
    pub fn refused(
        command: Option<&'static str>,
        reason: &str,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        let mut envelope = Self::error(command, ErrorCode::NodeRefused, warnings, meta);
        envelope.error = envelope.error.take().map(|error| error.with_reason(reason));
        envelope
    }

    /// An argument error caused by the value of one input the program checked
    /// itself, named by `reason` (today only [`INVALID_INVOICE`]). Data is null;
    /// the message stays the fixed text of `invalid_arguments`.
    pub fn invalid_input(
        command: Option<&'static str>,
        reason: &str,
        warnings: Warnings,
        meta: ExecutionMeta<S>,
    ) -> Self {
        let mut envelope = Self::error(command, ErrorCode::InvalidArguments, warnings, meta);
        envelope.error = envelope.error.take().map(|error| error.with_reason(reason));
        envelope
    }

    /// Attach the work numbers of a `pow_required` error; any other outcome,
    /// or `None`, is returned unchanged.
    pub fn with_work(mut self, work: Option<PowShortfall>) -> Self {
        if let Some(work) = work {
            self.error = self.error.take().map(|error| error.with_work(work));
        }
        self
    }

    /// The work numbers, only on `pow_required`.
    pub fn work(&self) -> Option<PowShortfall> {
        self.error.as_ref().and_then(ErrorInfo::work)
    }

    /// Record `extra` as well, still one warning per code.
    pub fn add_warnings(&mut self, extra: &Warnings) {
        self.warnings.merge(extra);
    }

    /// The node's refusal reason, only on `node_refused`.
    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }

    pub fn reason(&self) -> Option<&str> {
        self.error.as_ref().and_then(ErrorInfo::reason)
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn command(&self) -> Option<&'static str> {
        self.command
    }

    pub fn data(&self) -> Option<&D> {
        self.data.as_ref()
    }

    pub fn snapshot(&self) -> Option<&S> {
        self.meta.snapshot.as_ref()
    }

    pub fn error_code(&self) -> Option<ErrorCode> {
        self.error.as_ref().map(ErrorInfo::code)
    }

    /// An `internal_error` outcome for the same command, stages and warnings,
    /// without data or snapshot, for when this outcome cannot be serialized.
    pub fn as_internal_error(&self) -> Envelope<(), ()> {
        Envelope::error(
            self.command,
            ErrorCode::InternalError,
            self.warnings.clone(),
            ExecutionMeta {
                publication: self.meta.publication,
                daemon_acceptance: self.meta.daemon_acceptance,
                persistence: self.meta.persistence,
                snapshot: None,
            },
        )
    }

    pub fn exit_code(&self) -> i32 {
        self.error
            .as_ref()
            .map_or(0, |error| error.code().exit_code())
    }
}

#[cfg(test)]
mod tests;
