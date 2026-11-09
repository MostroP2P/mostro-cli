use clap::builder::PossibleValuesParser;
use clap::{ArgAction, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

pub use super::command_args::{
    AddBondInvoiceArgs, AddInvoiceArgs, ChatArgs, ConfigSetArgs, ListMessagesArgs, NewArgs,
    RateArgs, StepArgs, SyncArgs, TakeBuyArgs, TakeSellArgs,
};
use crate::outcome::order::{SIDES, STATUSES};

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum Commands {
    /// Public order book commands
    #[command(arg_required_else_help = true)]
    Order {
        #[command(subcommand)]
        command: OrderCommands,
    },
    /// Describe the machine interface offline: commands, inputs, results and
    /// errors. Needs no node, identity or network
    Schema,
    /// Slashed-bond commands
    #[command(arg_required_else_help = true)]
    Bond {
        #[command(subcommand)]
        command: BondCommands,
    },
    /// Messages the node sent you
    #[command(arg_required_else_help = true)]
    Message {
        #[command(subcommand)]
        command: MessageCommands,
    },
    /// The local identity: create it or show its public key. Needs no node,
    /// relay or network
    #[command(arg_required_else_help = true)]
    Identity {
        #[command(subcommand)]
        command: IdentityCommands,
    },
    /// Disputes on orders you hold
    #[command(arg_required_else_help = true)]
    Dispute {
        #[command(subcommand)]
        command: DisputeCommands,
    },
    /// The node settings file ($MCLI_DIR/config, else ~/.mcli/config). Needs
    /// no node, relay or network
    #[command(arg_required_else_help = true)]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
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
    /// Restore session to recover all pending orders and disputes
    Restore {},
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
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum OrderCommands {
    /// List public orders from the node's order book (read-only)
    List {
        /// Keep only this side: buy or sell (case-insensitive)
        #[arg(long, ignore_case = true, value_parser = PossibleValuesParser::new(SIDES))]
        side: Option<String>,
        /// Keep only this fiat currency code (case-insensitive)
        #[arg(long)]
        currency: Option<String>,
        /// Keep only this status (case-insensitive); defaults to pending
        #[arg(long, ignore_case = true, value_parser = PossibleValuesParser::new(STATUSES))]
        status: Option<String>,
    },
    /// Ask the node for its record of your own orders. Needs an existing local
    /// identity (it never creates one) and publishes one signed request
    Info {
        /// Order id to ask about; repeat the option for several orders
        #[arg(long = "order", required = true, value_name = "UUID")]
        orders: Vec<Uuid>,
    },
    /// Create a new buy or sell order. Needs an existing local identity (it
    /// never creates one), publishes one signed request and saves the order the
    /// node creates. JSON output requires --yes
    New(NewArgs),
    /// Take a sell order (you buy sats). Needs an existing local identity (it
    /// never creates one), publishes one signed request and saves the trade.
    /// JSON output requires --yes
    #[command(name = "take-sell")]
    TakeSell(TakeSellArgs),
    /// Take a buy order (you sell sats). Needs an existing local identity (it
    /// never creates one), publishes one signed request and saves the trade.
    /// JSON output requires --yes
    #[command(name = "take-buy")]
    TakeBuy(TakeBuyArgs),
    /// Send your invoice for an order you hold, as the node asked. Needs an
    /// existing local identity (it never creates one), publishes one signed
    /// request and updates the order's local status. JSON output requires --yes
    #[command(name = "add-invoice")]
    AddInvoice(AddInvoiceArgs),
    /// Tell the node you sent the fiat for an order you are buying. Needs an
    /// existing local identity (it never creates one), publishes one signed
    /// request and updates the order's local status. JSON output requires --yes
    #[command(name = "fiat-sent")]
    FiatSent(StepArgs),
    /// Release the sats of an order you are selling: the node settles the hold
    /// invoice and pays the buyer. Irreversible. Needs an existing local
    /// identity (it never creates one), publishes one signed request and
    /// updates the order's local status. JSON output requires --yes
    Release(StepArgs),
    /// Cancel an order, or start or confirm a cooperative cancel of an active
    /// trade: the node decides which and the result says so. Needs an existing
    /// local identity (it never creates one), publishes one signed request and
    /// updates the order's local status. JSON output requires --yes
    Cancel(StepArgs),
    /// Give the orders the node announced to your trade keys a local row: the
    /// child order of a range order, or any order the store lost. Reads the
    /// node's `new-order` messages like `message list`, saves a row only for an
    /// order the store does not know and publishes nothing. Needs an existing
    /// local identity (it never creates one)
    Sync(SyncArgs),
    /// List the orders of the local store (made, taken or adopted), newest
    /// first, with their trade index and key. Reads the local store only;
    /// needs no node, relay or network
    Mine,
    /// Rate the counterparty of a finished trade you hold, from 1 to 5. Needs
    /// an existing local identity (it never creates one) and publishes one
    /// signed request. JSON output requires --yes
    Rate(RateArgs),
    /// Read the peer chat of an order you hold and, with --message, send one
    /// message to its counterparty. The counterparty's key comes from the
    /// order's row or the node's messages, never from you. Needs an existing
    /// local identity (it never creates one); human mode sends without asking
    Chat(ChatArgs),
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum DisputeCommands {
    /// Open a dispute on an active order you hold. Needs an existing local
    /// identity (it never creates one), publishes one signed request and
    /// updates the order's local status. JSON output requires --yes
    Open(StepArgs),
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum MessageCommands {
    /// Read what the node sent to your trade keys: the next step of each
    /// trade, with any invoice to pay as data. Read-only; needs an existing
    /// local identity (it never creates one)
    List(ListMessagesArgs),
}

#[derive(Subcommand, Clone, Debug)]
#[clap(rename_all = "lower")]
pub enum IdentityCommands {
    /// Create the local identity store (`MCLI_DIR`, else ~/.mcli) with a fresh
    /// mnemonic, as the first run of a legacy command does. Never contacts a
    /// relay and never shows the mnemonic; fails if a store already exists
    Init,
    /// Show the identity public key and the last trade index of the local
    /// store. Read-only; never shows the mnemonic
    Show,
}

#[derive(Subcommand, Clone, Debug)]
#[clap(rename_all = "lower")]
pub enum ConfigCommands {
    /// Write one node setting to the config file, keeping the others: the
    /// data directory is created if missing and the file is rewritten as
    /// NAME=value lines (comments are dropped); on Unix both are made
    /// owner-only. A file the commands refuse is left untouched
    Set(ConfigSetArgs),
    /// Report the config file's path and the node settings stored in it
    /// (not set when absent), read-only; options and environment variables
    /// are not reported. A file the commands refuse is an error
    Show,
}

#[derive(Subcommand, Clone)]
#[clap(rename_all = "lower")]
pub enum BondCommands {
    /// Reply to a bond payout request with an invoice for your share of a
    /// slashed bond. Needs an existing local identity (it never creates one)
    /// and publishes one signed request. JSON output requires --yes
    #[command(name = "add-invoice")]
    AddInvoice(AddBondInvoiceArgs),
}
