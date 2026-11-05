use clap::builder::PossibleValuesParser;
use clap::{ArgAction, Args, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

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
}

#[derive(Args, Clone, Debug)]
pub struct NewArgs {
    /// The order's side: buy or sell (case-insensitive)
    #[arg(long, ignore_case = true, value_parser = PossibleValuesParser::new(SIDES))]
    pub side: String,
    /// Fiat currency code, such as USD
    #[arg(long)]
    pub currency: String,
    /// Fiat amount; with --fiat-amount-max, the range minimum
    #[arg(long)]
    pub fiat_amount: i64,
    /// Range maximum: makes a range order at market price
    #[arg(long)]
    pub fiat_amount_max: Option<i64>,
    /// Payment method text
    #[arg(long)]
    pub payment_method: String,
    /// Sats amount; 0 means the market price
    #[arg(long, default_value_t = 0)]
    pub sats: i64,
    /// Premium on the price, in percent
    #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
    pub premium: i64,
    /// Lightning invoice, when the order needs one
    #[arg(long)]
    pub invoice: Option<String>,
    /// Days until the pending order expires; 0 uses the node's default
    #[arg(long, default_value_t = 0)]
    pub expiration_days: i64,
    /// Create the order without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}
