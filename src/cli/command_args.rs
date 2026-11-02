//! The argument groups of the migrated commands that take more than a flag or
//! two, kept apart so `commands.rs` stays a list of commands.

use clap::Args;
use uuid::Uuid;

#[derive(Args, Clone, Debug)]
pub struct AddInvoiceArgs {
    /// Id of the order the node asked an invoice for
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// Your invoice (BOLT-11 or Lightning address) for the sats
    #[arg(long)]
    pub invoice: String,
    /// Send the invoice without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct AddBondInvoiceArgs {
    /// Id of the order whose bond was slashed
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// A BOLT-11 invoice for exactly your share of the slashed bond
    #[arg(long)]
    pub invoice: String,
    /// Send the invoice without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct ListMessagesArgs {
    /// How far back to read, in minutes (1 to 10080, default 30)
    #[arg(long, value_name = "MINUTES")]
    pub since: Option<i64>,
    /// Read only the messages of this order
    #[arg(long = "order", value_name = "UUID")]
    pub order: Option<Uuid>,
}

#[derive(Args, Clone, Debug)]
pub struct StepArgs {
    /// Id of the order
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// Send the request without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct SyncArgs {
    /// How far back to read, in minutes (1 to 10080, default 10080)
    #[arg(long, value_name = "MINUTES")]
    pub since: Option<i64>,
}
