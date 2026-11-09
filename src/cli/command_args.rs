//! The argument groups of the migrated commands that take more than a flag or
//! two, kept apart so `commands.rs` stays a list of commands.

use clap::builder::PossibleValuesParser;
use clap::Args;
use uuid::Uuid;

use crate::outcome::order::SIDES;
use crate::settings::SETTING_KEYS;

#[derive(Args, Clone, Debug)]
pub struct AddInvoiceArgs {
    /// Id of the order the node asked an invoice for
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// Your invoice (BOLT-11 or Lightning address) for the sats; `-` reads it
    /// from standard input: one line, then close standard input; needs --yes,
    /// and stdin must not be a terminal
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
    /// A BOLT-11 invoice for exactly your share of the slashed bond; `-` reads
    /// it from standard input: one line, then close standard input; needs
    /// --yes, and stdin must not be a terminal
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

#[derive(Args, Clone, Debug)]
pub struct TakeSellArgs {
    /// Id of the sell order to take
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// Your invoice (BOLT-11 or Lightning address) for the sats; without one
    /// the node asks for it next. `-` reads it from standard input: one line,
    /// then close standard input; needs --yes, and stdin must not be a terminal
    #[arg(long)]
    pub invoice: Option<String>,
    /// Fiat amount to buy, for a range order
    #[arg(long)]
    pub amount: Option<i64>,
    /// Take the order without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct TakeBuyArgs {
    /// Id of the buy order to take
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// Fiat amount to sell, for a range order
    #[arg(long)]
    pub amount: Option<i64>,
    /// Take the order without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
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
    /// Lightning invoice, when the order needs one; `-` reads it from standard
    /// input: one line, then close standard input; needs --yes, and stdin must
    /// not be a terminal
    #[arg(long)]
    pub invoice: Option<String>,
    /// Days until the pending order expires; 0 uses the node's default
    #[arg(long, default_value_t = 0)]
    pub expiration_days: i64,
    /// Seconds until the pending order expires (1 to 31536000), instead of
    /// --expiration-days; the node lowers it to its own maximum
    #[arg(long, value_name = "SECONDS", conflicts_with = "expiration_days")]
    pub expires_in: Option<i64>,
    /// Create the order without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct RateArgs {
    /// Id of the finished order whose counterparty you rate
    #[arg(long = "order", value_name = "UUID")]
    pub order: Uuid,
    /// The rating, from 1 to 5
    #[arg(long, value_name = "1-5")]
    pub rating: u8,
    /// Send the rating without asking (required with --output json)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug)]
pub struct ConfigSetArgs {
    /// The setting: mostro-pubkey (MOSTRO_PUBKEY), relays (RELAYS), pow (POW)
    /// or transport (TRANSPORT)
    #[arg(value_parser = PossibleValuesParser::new(SETTING_KEYS))]
    pub key: String,
    /// The value, trimmed and checked as the commands check it: a public key
    /// (hex or npub), comma-separated ws:// or wss:// relay URLs, a number
    /// from 0 to 255, or nip44; put `--` before a value that starts with `-`
    pub value: String,
}
