use clap::{ArgAction, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

use super::args::check_fiat_range;
use crate::outcome::ExecutionMeta;

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum Commands {
    /// Requests open orders from Mostro pubkey
    ListOrders {
        /// Status of the order
        #[arg(short, long)]
        status: Option<String>,
        /// Currency selected
        #[arg(short, long)]
        currency: Option<String>,
        /// Choose an order kind
        #[arg(short, long)]
        kind: Option<String>,
    },
    /// Create a new buy/sell order on Mostro
    NewOrder {
        /// Choose an order kind
        #[arg(short, long)]
        kind: String,
        /// Sats amount - leave empty for market price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        amount: i64,
        /// Currency selected
        #[arg(short = 'c', long)]
        fiat_code: String,
        /// Fiat amount
        #[arg(short, long)]
        #[clap(value_parser=check_fiat_range)]
        fiat_amount: (i64, Option<i64>),
        /// Payment method
        #[arg(short = 'm', long)]
        payment_method: String,
        /// Premium on price
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        #[clap(allow_hyphen_values = true)]
        premium: i64,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
        /// Expiration time of a pending Order, in days
        #[arg(short, long)]
        #[clap(default_value_t = 0)]
        expiration_days: i64,
    },
    /// Take a sell order from a Mostro pubkey
    TakeSell {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: Option<String>,
        /// Amount of fiat to pay (someone is selling sats, you are buying)
        #[arg(short, long)]
        amount: Option<u32>,
    },
    /// Take a buy order from a Mostro pubkey
    TakeBuy {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Amount of fiat to receive (someone is buying sats, you are selling)
        #[arg(short, long)]
        amount: Option<u32>,
    },
    /// Buyer add a new invoice to receive the payment
    AddInvoice {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
    /// Reply to a bond payout request with an invoice for your share of a slashed bond
    AddBondInvoice {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Invoice string
        #[arg(short, long)]
        invoice: String,
    },
    /// Get the latest direct messages
    GetDm {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
        /// If true, get messages from counterparty, otherwise from Mostro
        #[arg(short, long)]
        from_user: bool,
    },
    /// Read both sides of the kind-14 chat for an order
    GetDmUser {
        /// Counterparty pubkey the conversation is derived from
        #[arg(short, long)]
        pubkey: String,
        /// Order id to get the trade keys from
        #[arg(short, long)]
        order_id: Uuid,
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
    },
    /// Read the dispute conversation with the solver handling your order
    DisputeChat {
        /// Order id that is in dispute
        #[arg(short, long)]
        order_id: Uuid,
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 1440)]
        since: i64,
        /// Send this message to the solver instead of only reading
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Get the latest direct messages for admin
    GetAdminDm {
        /// Since time of the messages in minutes
        #[arg(short, long)]
        #[clap(default_value_t = 30)]
        since: i64,
        /// If true, get messages from counterparty, otherwise from Mostro
        #[arg(short, long)]
        from_user: bool,
    },
    /// Send direct message to a user
    SendDm {
        /// Pubkey of the counterpart
        #[arg(short, long)]
        pubkey: String,
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Message to send (spaces allowed; use quotes or multiple -m/--message)
        #[arg(short, long, num_args = 1..)]
        message: Vec<String>,
    },
    /// Send a kind-14 chat message to a user
    DmToUser {
        /// Pubkey of the recipient
        #[arg(short, long)]
        pubkey: String,
        /// Order id to get ephemeral keys
        #[arg(short, long)]
        order_id: Uuid,
        /// Message to send (spaces allowed; use quotes or multiple -m/--message)
        #[arg(short, long, num_args = 1..)]
        message: Vec<String>,
    },
    /// Send fiat sent message to confirm payment to other user
    FiatSent {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Settle the hold invoice and pay to buyer.
    Release {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Cancel a pending order
    Cancel {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Rate counterpart after a successful trade
    Rate {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Rating from 1 to 5
        #[arg(short, long)]
        rating: u8,
    },
    /// Restore session to recover all pending orders and disputes
    Restore {},
    /// Start a dispute
    Dispute {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Cancel an order (only admin)
    AdmCancel {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Slash the seller's bond (anti-abuse-bond Phase 2)
        #[arg(long, default_value_t = false)]
        slash_seller: bool,
        /// Slash the buyer's bond (anti-abuse-bond Phase 2)
        #[arg(long, default_value_t = false)]
        slash_buyer: bool,
    },
    /// Settle a seller's hold invoice (only admin)
    AdmSettle {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
        /// Slash the seller's bond (anti-abuse-bond Phase 2)
        #[arg(long, default_value_t = false)]
        slash_seller: bool,
        /// Slash the buyer's bond (anti-abuse-bond Phase 2)
        #[arg(long, default_value_t = false)]
        slash_buyer: bool,
    },
    /// Requests open disputes from Mostro pubkey
    ListDisputes {},
    /// Enable/disable the daemon's maintenance (drain) mode over the admin
    /// gRPC (only operator; needs MOSTRO_RPC_URL / MOSTRO_RPC_TOKEN, not
    /// ADMIN_NSEC). While ON, new orders and takes are rejected; open trades
    /// keep working so escrow can drain before a Lightning node migration.
    AdmSetMaintenance {
        /// true to enter maintenance mode, false to leave it
        #[arg(short, long, action = ArgAction::Set)]
        enabled: bool,
        /// Free-text reason stored with the flag (never published)
        #[arg(short, long)]
        reason: Option<String>,
    },
    /// Show the maintenance flag and what is still bound to the daemon's
    /// Lightning node; poll until `drained = true` before switching nodes
    AdmMaintenanceStatus {},
    /// Cancel a still-pending (or waiting-taker-bond) order as the operator
    /// over the admin gRPC: the maker is notified and every bond on it is
    /// released at once. Shortens the maintenance drain. Only operator;
    /// needs MOSTRO_RPC_URL / MOSTRO_RPC_TOKEN, not ADMIN_NSEC. Unlike
    /// `admcancel` this is not a dispute resolution.
    AdmCancelPending {
        /// Order id
        #[arg(short, long)]
        order_id: Uuid,
    },
    /// Add a new dispute's solver (only admin)
    AdmAddSolver {
        /// Solver pubkey (npub or hex), optionally suffixed with `:read`, `:read-write` or `:write`
        #[arg(short, long)]
        npubkey: String,
    },
    /// Admin or solver take a Pending dispute (only admin)
    AdmTakeDispute {
        /// Dispute id
        #[arg(short, long)]
        dispute_id: Uuid,
    },
    /// Send a direct message to a user (only admin)
    AdmSendDm {
        /// Pubkey of the recipient
        #[arg(short, long)]
        pubkey: String,
        /// Message to send (spaces allowed; use quotes or multiple -m/--message)
        #[arg(short, long, num_args = 1..)]
        message: Vec<String>,
    },
    /// Send admin DM with encrypted attachment to a Blossom server
    SendAdminDmAttach {
        /// Pubkey of the admin recipient
        #[arg(short, long)]
        pubkey: String,
        /// Order id to derive the correct trade key
        #[arg(short, long)]
        order_id: Uuid,
        /// Path to file to encrypt and upload
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Get the conversation key for direct messaging with a user
    ConversationKey {
        /// Pubkey of the counterpart
        #[arg(short, long)]
        pubkey: String,
    },
    /// Get last trade index of user
    GetLastTradeIndex {},
    /// Get private key of last trade index public key
    GetLastTradePrivkey {},
    /// Get your reputation (rating, reviews, first trade) as Mostro holds it
    GetUserInfo {},
    /// Request detailed information for specific orders
    OrdersInfo {
        /// Order IDs to request information for
        #[arg(short, long)]
        order_ids: Vec<Uuid>,
    },
}

/// What a registered command supports with `--output json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonSupport {
    /// Human output only; JSON is rejected with `output_not_supported`.
    Unsupported,
    /// A read-only command whose JSON outcomes use read stages.
    Read,
}

/// One entry of the command registry, the single source of canonical names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    /// The subcommand as it is typed today.
    pub cli_name: &'static str,
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
        }
    }
}

const fn legacy(cli_name: &'static str, canonical: &'static str) -> CommandEntry {
    CommandEntry {
        cli_name,
        canonical,
        json: JsonSupport::Unsupported,
    }
}

/// Every subcommand in declaration order. Canonical names are part of the
/// JSON contract: changing one requires a new contract version.
pub const REGISTRY: &[CommandEntry] = &[
    legacy("listorders", "order.list"),
    legacy("neworder", "order.new"),
    legacy("takesell", "order.take-sell"),
    legacy("takebuy", "order.take-buy"),
    legacy("addinvoice", "order.add-invoice"),
    legacy("addbondinvoice", "bond.add-invoice"),
    legacy("getdm", "message.list"),
    legacy("getdmuser", "chat.history"),
    legacy("disputechat", "dispute.chat"),
    legacy("getadmindm", "admin.message-list"),
    legacy("senddm", "message.send"),
    legacy("dmtouser", "chat.send"),
    legacy("fiatsent", "order.fiat-sent"),
    legacy("release", "order.release"),
    legacy("cancel", "order.cancel"),
    legacy("rate", "order.rate"),
    legacy("restore", "account.restore"),
    legacy("dispute", "dispute.open"),
    legacy("admcancel", "admin.cancel"),
    legacy("admsettle", "admin.settle"),
    legacy("listdisputes", "dispute.list"),
    legacy("admsetmaintenance", "maintenance.set"),
    legacy("admmaintenancestatus", "maintenance.status"),
    legacy("admcancelpending", "maintenance.cancel-pending"),
    legacy("admaddsolver", "admin.add-solver"),
    legacy("admtakedispute", "admin.take-dispute"),
    legacy("admsenddm", "admin.message-send"),
    legacy("sendadmindmattach", "admin.message-attach"),
    legacy("conversationkey", "account.conversation-key"),
    legacy("getlasttradeindex", "account.trade-index"),
    legacy("getlasttradeprivkey", "account.trade-key"),
    legacy("getuserinfo", "account.info"),
    legacy("ordersinfo", "order.info"),
];

/// The entry for a resolved command path such as `["neworder"]`.
pub fn lookup(path: &[String]) -> Option<&'static CommandEntry> {
    match path {
        [name] => REGISTRY.iter().find(|entry| entry.cli_name == name),
        _ => None,
    }
}
