//! Command registry: canonical names and JSON capability per command path.

use crate::outcome::{DaemonAcceptance, ExecutionMeta, Publication};

/// What a registered command supports with `--output json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonSupport {
    /// Human output only; JSON is rejected with `output_not_supported`.
    Unsupported,
    /// A read-only command whose JSON outcomes use read stages.
    Read,
    /// A read that publishes one request to the node and writes nothing
    /// locally; its stages tell whether the request was published.
    Request,
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
    legacy(&["neworder"], "order.new"),
    legacy(&["takesell"], "order.take-sell"),
    legacy(&["takebuy"], "order.take-buy"),
    legacy(&["addinvoice"], "order.add-invoice"),
    legacy(&["addbondinvoice"], "bond.add-invoice"),
    legacy(&["getdm"], "message.list"),
    legacy(&["getdmuser"], "chat.history"),
    legacy(&["disputechat"], "dispute.chat"),
    legacy(&["getadmindm"], "admin.message-list"),
    legacy(&["senddm"], "message.send"),
    legacy(&["dmtouser"], "chat.send"),
    legacy(&["fiatsent"], "order.fiat-sent"),
    legacy(&["release"], "order.release"),
    legacy(&["cancel"], "order.cancel"),
    legacy(&["rate"], "order.rate"),
    legacy(&["restore"], "account.restore"),
    legacy(&["dispute"], "dispute.open"),
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
    legacy(&["ordersinfo"], "order.info"),
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
