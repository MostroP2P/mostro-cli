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
    /// A refusal for a stale index keeps the index spent: `applied`.
    Write,
    /// One published request that updates a local row only afterwards:
    /// `applied`, or `failed` on success (the outcome then warns), and
    /// `not_attempted` when the reply called for no update or the outcome is
    /// not a success.
    Update,
    /// One published request with no local write at all: persistence is always
    /// `not_attempted`.
    Delivery,
    /// One published request on an order the user holds: as [`Stages::Update`],
    /// and an unconfirmed request on a range order also records the trade index
    /// it reserved (`applied` or `failed`).
    Step,
    /// A read of the relays that may write rows locally and publishes nothing:
    /// publication and node acceptance are `not_applicable`; persistence is
    /// `applied` when a row was saved, `not_attempted` when nothing was to be
    /// saved or the command stopped first, `failed` when a save failed.
    Sync,
    /// A command that only writes local state and talks to nobody
    /// (`identity.init`): publication and node acceptance are `not_applicable`;
    /// persistence is `applied` on success, `not_attempted` when nothing was
    /// written, `failed` when a write began and could not be completed, and
    /// `not_applicable` for an invocation rejected before it ran, as for
    /// [`Stages::Sync`].
    Local,
    /// `order.chat`: a read of the relays that may publish one chat message
    /// to the counterparty, never to the node. An amendment of the general
    /// meanings for this command: publication is `not_applicable` until the
    /// send step runs, with or without `--message` (a rejected invocation, the
    /// missing `--yes`, configuration, identity, store, `order_unknown`,
    /// `counterparty_unknown`, a failed lookup or chat read), because the
    /// registry reports a rejected invocation as a read before `--message` is
    /// known. The send step starts at the wrap and reports only its own
    /// result: `published`; `not_attempted` when the wrap failed
    /// (`internal_error`), the relays could not be connected (the connect code)
    /// or every relay refused (`request_rejected`); `unknown` (`uncertain`)
    /// when no relay confirmed and one failed ambiguously. The node is not
    /// involved, so acceptance is always `not_applicable`, and so is
    /// persistence: the counterparty key the lookup may cache in the order row
    /// is not reported.
    Chat,
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
    /// The reasons an `invalid_arguments` may carry (`invalid_invoice` for the
    /// commands that take an invoice); empty for none.
    pub input_reasons: &'a [&'static str],
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
            input_reasons: &[],
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
            input_reasons: &[],
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
            input_reasons: &[],
            stages: Stages::Write,
        }
    }

    /// Rules of `order.add-invoice` (`Stages::Update`) or `bond.add-invoice`
    /// (`Stages::Delivery`): one published request, no trade index.
    pub fn invoice(
        command: &'static str,
        stages: Stages,
        errors: &'a [ErrorCode],
        uncertain: &'a [ErrorCode],
        warnings: &'a [WarningCode],
    ) -> Self {
        Self {
            stages,
            ..Self::write(command, errors, uncertain, warnings)
        }
    }

    /// Rules of `order.sync` (`Stages::Sync`): a read with local rows.
    pub fn sync(
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
            snapshot_on_success: true,
            reason: false,
            input_reasons: &[],
            stages: Stages::Sync,
        }
    }

    /// Rules of a local write that publishes nothing (`Stages::Local`).
    pub fn local(command: &'static str, errors: &'a [ErrorCode]) -> Self {
        Self {
            command,
            errors,
            uncertain: &[],
            warnings: &[],
            snapshot_on_success: false,
            reason: false,
            input_reasons: &[],
            stages: Stages::Local,
        }
    }

    /// Rules of the trade steps (`Stages::Step`).
    pub fn step(
        command: &'static str,
        errors: &'a [ErrorCode],
        uncertain: &'a [ErrorCode],
        warnings: &'a [WarningCode],
    ) -> Self {
        Self {
            stages: Stages::Step,
            ..Self::write(command, errors, uncertain, warnings)
        }
    }

    /// Rules of `order.chat` (`Stages::Chat`).
    pub fn chat(
        command: &'static str,
        errors: &'a [ErrorCode],
        uncertain: &'a [ErrorCode],
    ) -> Self {
        Self {
            command,
            errors,
            uncertain,
            warnings: &[],
            snapshot_on_success: false,
            reason: false,
            input_reasons: &[],
            stages: Stages::Chat,
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
            // Read only, read and sent, or the send step's own result.
            Stages::Chat => Combos {
                success: vec![
                    ["not_applicable"; 3],
                    ["published", "not_applicable", "not_applicable"],
                ],
                failure: vec![
                    ["not_applicable"; 3],
                    ["not_attempted", "not_applicable", "not_applicable"],
                ],
                uncertain: vec![["unknown", "not_applicable", "not_applicable"]],
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
                // Nothing sent, or refused (the index given back, or kept on
                // `invalid_trade_index`).
                failure: vec![
                    ["not_attempted", "unknown", "not_attempted"],
                    ["published", "rejected", "not_attempted"],
                    ["published", "rejected", "applied"],
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
            Stages::Sync => Combos {
                success: vec![
                    ["not_applicable", "not_applicable", "applied"],
                    ["not_applicable", "not_applicable", "not_attempted"],
                ],
                // Before anything was read, or after reading and before saving.
                failure: vec![
                    ["not_applicable", "not_applicable", "not_applicable"],
                    ["not_applicable", "not_applicable", "not_attempted"],
                ],
                // A save failed after others succeeded.
                uncertain: vec![["not_applicable", "not_applicable", "failed"]],
            },
            Stages::Local => Combos {
                success: vec![["not_applicable", "not_applicable", "applied"]],
                // Rejected before it ran, ran and wrote nothing, or began a
                // write that could not be completed.
                failure: vec![
                    ["not_applicable", "not_applicable", "not_applicable"],
                    ["not_applicable", "not_applicable", "not_attempted"],
                    ["not_applicable", "not_applicable", "failed"],
                ],
                uncertain: Vec::new(),
            },
            Stages::Update | Stages::Delivery | Stages::Step => Combos {
                success: if self.stages != Stages::Delivery {
                    vec![
                        ["published", "accepted", "applied"],
                        ["published", "accepted", "failed"],
                        ["published", "accepted", "not_attempted"],
                    ]
                } else {
                    vec![["published", "accepted", "not_attempted"]]
                },
                failure: vec![
                    ["not_attempted", "unknown", "not_attempted"],
                    ["published", "rejected", "not_attempted"],
                ],
                // The node may hold the invoice: no reply, an unusable reply
                // or an unconfirmed publication.
                uncertain: if self.stages == Stages::Step {
                    vec![
                        ["published", "unknown", "not_attempted"],
                        ["published", "unknown", "applied"],
                        ["published", "unknown", "failed"],
                        ["unknown", "unknown", "not_attempted"],
                        ["unknown", "unknown", "applied"],
                        ["unknown", "unknown", "failed"],
                    ]
                } else {
                    vec![
                        ["published", "unknown", "not_attempted"],
                        ["unknown", "unknown", "not_attempted"],
                    ]
                },
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
    for code in own {
        if !codes.contains(code) {
            codes.push(*code);
        }
    }
    Value::Array(codes.iter().map(value).collect())
}

pub(super) fn value<T: serde::Serialize>(item: &T) -> Value {
    serde_json::to_value(item).unwrap_or(Value::Null)
}

pub(super) fn code_names(codes: &[ErrorCode]) -> Vec<Value> {
    codes.iter().map(value).collect()
}
