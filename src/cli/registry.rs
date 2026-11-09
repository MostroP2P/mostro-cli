//! Command registry: canonical names and JSON capability per command path.

use crate::outcome::{DaemonAcceptance, ExecutionMeta, Persistence, Publication};

/// What a registered command supports with `--output json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonSupport {
    /// Human output only; JSON is rejected with `output_not_supported`.
    Unsupported,
    /// A command that sends the node no request: an outcome produced before
    /// it runs uses read stages. Not always read-only: `order chat` with
    /// `--message` publishes to the peer and reports that publication from
    /// its send step on (spec 019, D4).
    Read,
    /// A read that publishes one request to the node and writes nothing
    /// locally; its stages tell whether the request was published.
    Request,
    /// A command that publishes one request and writes locally; its stages
    /// also say whether the local write happened.
    Write,
}

/// One entry of the command registry, the single source of canonical names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    /// The subcommand path as it is typed, such as `["order", "list"]`.
    pub path: &'static [&'static str],
    /// Dotted name reported in the `command` field of JSON outcomes.
    pub canonical: &'static str,
    pub json: JsonSupport,
}

impl CommandEntry {
    /// Stages of an outcome produced before this command executed.
    pub fn stages<S>(&self) -> ExecutionMeta<S> {
        match self.json {
            JsonSupport::Unsupported => ExecutionMeta::not_executed(),
            JsonSupport::Read => ExecutionMeta::read(None),
            JsonSupport::Request => {
                ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown)
            }
            JsonSupport::Write => ExecutionMeta::write(
                Publication::NotAttempted,
                DaemonAcceptance::Unknown,
                Persistence::NotAttempted,
            ),
        }
    }
}

const fn legacy(path: &'static [&'static str], canonical: &'static str) -> CommandEntry {
    CommandEntry {
        path,
        canonical,
        json: JsonSupport::Unsupported,
    }
}

/// Every leaf command in declaration order. Canonical names are part of the
/// JSON contract: changing one requires a new contract version.
pub const REGISTRY: &[CommandEntry] = &[
    CommandEntry {
        path: &["order", "list"],
        canonical: "order.list",
        json: JsonSupport::Read,
    },
    CommandEntry {
        path: &["schema"],
        canonical: "schema",
        json: JsonSupport::Read,
    },
    CommandEntry {
        path: &["message", "list"],
        canonical: "message.list",
        json: JsonSupport::Read,
    },
    legacy(&["getdmuser"], "chat.history"),
    legacy(&["disputechat"], "dispute.chat"),
    legacy(&["getadmindm"], "admin.message-list"),
    legacy(&["senddm"], "message.send"),
    legacy(&["dmtouser"], "chat.send"),
    legacy(&["restore"], "account.restore"),
    legacy(&["admcancel"], "admin.cancel"),
    legacy(&["admsettle"], "admin.settle"),
    legacy(&["listdisputes"], "dispute.list"),
    legacy(&["admsetmaintenance"], "maintenance.set"),
    legacy(&["admmaintenancestatus"], "maintenance.status"),
    legacy(&["admcancelpending"], "maintenance.cancel-pending"),
    legacy(&["admaddsolver"], "admin.add-solver"),
    legacy(&["admtakedispute"], "admin.take-dispute"),
    legacy(&["admsenddm"], "admin.message-send"),
    legacy(&["sendadmindmattach"], "admin.message-attach"),
    legacy(&["conversationkey"], "account.conversation-key"),
    legacy(&["getlasttradeindex"], "account.trade-index"),
    legacy(&["getlasttradeprivkey"], "account.trade-key"),
    legacy(&["getuserinfo"], "account.info"),
    CommandEntry {
        path: &["order", "info"],
        canonical: "order.info",
        json: JsonSupport::Request,
    },
    CommandEntry {
        path: &["order", "new"],
        canonical: "order.new",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "take-sell"],
        canonical: "order.take-sell",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "take-buy"],
        canonical: "order.take-buy",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "add-invoice"],
        canonical: "order.add-invoice",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "fiat-sent"],
        canonical: "order.fiat-sent",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "release"],
        canonical: "order.release",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["order", "cancel"],
        canonical: "order.cancel",
        json: JsonSupport::Write,
    },
    // Publishes nothing and reads like a read; it only saves rows locally.
    CommandEntry {
        path: &["order", "sync"],
        canonical: "order.sync",
        json: JsonSupport::Read,
    },
    CommandEntry {
        path: &["bond", "add-invoice"],
        canonical: "bond.add-invoice",
        json: JsonSupport::Write,
    },
    // Local only: they publish nothing, so a rejected invocation reports read
    // stages, as `order sync` does.
    CommandEntry {
        path: &["identity", "init"],
        canonical: "identity.init",
        json: JsonSupport::Read,
    },
    CommandEntry {
        path: &["identity", "show"],
        canonical: "identity.show",
        json: JsonSupport::Read,
    },
    // Reads the local store only.
    CommandEntry {
        path: &["order", "mine"],
        canonical: "order.mine",
        json: JsonSupport::Read,
    },
    CommandEntry {
        path: &["order", "rate"],
        canonical: "order.rate",
        json: JsonSupport::Write,
    },
    CommandEntry {
        path: &["dispute", "open"],
        canonical: "dispute.open",
        json: JsonSupport::Write,
    },
    // Writes the config file only and publishes nothing, like `identity init`.
    CommandEntry {
        path: &["config", "set"],
        canonical: "config.set",
        json: JsonSupport::Read,
    },
    // Reads the config file only, like `identity show`.
    CommandEntry {
        path: &["config", "show"],
        canonical: "config.show",
        json: JsonSupport::Read,
    },
    // The peer chat of an order held. Its publication stays `not_applicable`
    // until the send step runs, so a rejected invocation reports read stages
    // even with `--message`; it never sends the node anything.
    CommandEntry {
        path: &["order", "chat"],
        canonical: "order.chat",
        json: JsonSupport::Read,
    },
    // Follows one order live; human output only for now (spec 020, unit 5).
    CommandEntry {
        path: &["order", "watch"],
        canonical: "order.watch",
        json: JsonSupport::Unsupported,
    },
];

/// The entry for a complete leaf path such as `["order", "list"]`.
pub fn lookup(path: &[String]) -> Option<&'static CommandEntry> {
    REGISTRY
        .iter()
        .find(|entry| entry.path.iter().eq(path.iter()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_command_rejected_before_it_runs_reports_nothing_sent() {
        let entry = CommandEntry {
            path: &["order", "info"],
            canonical: "order.info",
            json: JsonSupport::Request,
        };
        assert_eq!(
            entry.stages::<()>(),
            ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown)
        );
        // It differs from both a read (nothing applies) and a legacy rejection
        // (persistence not attempted).
        assert_ne!(entry.stages::<()>(), ExecutionMeta::read(None));
        assert_ne!(entry.stages::<()>(), ExecutionMeta::not_executed());
    }
}
