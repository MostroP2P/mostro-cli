//! What a command's outcomes may contain: the allowed codes, warnings and
//! stage combinations that its generated output schema is built from.

use serde_json::Value;

use super::{ErrorCode, WarningCode};

/// The stage vocabulary of a command's outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stages {
    /// A read that publishes nothing: every stage is `not_applicable`.
    Read,
    /// One published request and no local write: publication and node
    /// acceptance vary, persistence is `not_applicable`.
    Request,
    /// One published request and a local write: persistence says whether the
    /// write the outcome calls for was `applied` or `failed`, and is
    /// `not_attempted` when no write is called for (a refusal, a local failure).
    Write,
}

/// The stage combinations (publication, node acceptance, persistence) allowed
/// per status of the resolved command.
pub(super) struct Combos {
    pub success: Vec<[&'static str; 3]>,
    pub failure: Vec<[&'static str; 3]>,
    pub uncertain: Vec<[&'static str; 3]>,
}

/// What a command's outcomes may contain.
pub struct OutcomeRules<'a> {
    /// Canonical name reported on success.
    pub command: &'static str,
    /// Codes that can fail this command after it resolves.
    pub errors: &'a [ErrorCode],
    /// Codes that can leave the outcome `uncertain` (the request may have
    /// reached the node). Empty for commands that never emit that status.
    pub uncertain: &'a [ErrorCode],
    /// Warning codes beyond the frozen pilot pair that this command can emit.
    pub warnings: &'a [WarningCode],
    /// Whether a success always carries snapshot evidence.
    pub snapshot_on_success: bool,
    /// Whether `node_refused` may carry a `reason`.
    pub reason: bool,
    pub stages: Stages,
}

impl<'a> OutcomeRules<'a> {
    /// Rules of a read command with the pilot's vocabulary.
    pub fn read(command: &'static str, errors: &'a [ErrorCode], snapshot_on_success: bool) -> Self {
        Self {
            command,
            errors,
            uncertain: &[],
            warnings: &[],
            snapshot_on_success,
            reason: false,
            stages: Stages::Read,
        }
    }

    /// Rules of a command that publishes one request.
    pub fn request(
        command: &'static str,
        errors: &'a [ErrorCode],
        warnings: &'a [WarningCode],
    ) -> Self {
        Self {
            command,
            errors,
            uncertain: &[],
            warnings,
            snapshot_on_success: false,
            reason: true,
            stages: Stages::Request,
        }
    }

    /// Rules of a command that publishes one request and writes locally.
    pub fn write(
        command: &'static str,
        errors: &'a [ErrorCode],
        uncertain: &'a [ErrorCode],
        warnings: &'a [WarningCode],
    ) -> Self {
        Self {
            command,
            errors,
            uncertain,
            warnings,
            snapshot_on_success: false,
            reason: true,
            stages: Stages::Write,
        }
    }

    /// Stage combinations allowed on success, failure and uncertainty of the
    /// resolved command.
    pub(super) fn combos(&self) -> Combos {
        match self.stages {
            Stages::Read => Combos {
                success: vec![["not_applicable"; 3]],
                failure: vec![["not_applicable"; 3]],
                uncertain: Vec::new(),
            },
            Stages::Request => Combos {
                success: vec![["published", "accepted", "not_applicable"]],
                failure: vec![
                    ["not_attempted", "unknown", "not_applicable"],
                    ["published", "unknown", "not_applicable"],
                    ["published", "rejected", "not_applicable"],
                    ["unknown", "unknown", "not_applicable"],
                ],
                uncertain: Vec::new(),
            },
            Stages::Write => Combos {
                success: vec![["published", "accepted", "applied"]],
                // Nothing sent; refused; or accepted but not live (bond), with
                // the index recorded or not.
                failure: vec![
                    ["not_attempted", "unknown", "not_attempted"],
                    ["published", "rejected", "not_attempted"],
                    ["published", "accepted", "applied"],
                    ["published", "accepted", "failed"],
                ],
                // The node may hold an order: unknown or accepted without a
                // saved row, with the index recorded or not.
                uncertain: vec![
                    ["published", "unknown", "applied"],
                    ["published", "unknown", "failed"],
                    ["unknown", "unknown", "applied"],
                    ["unknown", "unknown", "failed"],
                    ["published", "accepted", "failed"],
                ],
            },
        }
    }
}

/// The `$defs` enum of a shared code type: the frozen pilot codes, then the
/// command's own in the order given.
pub(super) fn scoped_codes<T: serde::Serialize + PartialEq + Copy>(
    pilot: &[T],
    own: &[T],
) -> Value {
    let mut codes: Vec<T> = pilot.to_vec();
    codes.extend(own.iter().filter(|code| !pilot.contains(code)));
    Value::Array(codes.iter().map(value).collect())
}

pub(super) fn value<T: serde::Serialize>(item: &T) -> Value {
    serde_json::to_value(item).unwrap_or(Value::Null)
}

pub(super) fn code_names(codes: &[ErrorCode]) -> Vec<Value> {
    codes.iter().map(value).collect()
}
