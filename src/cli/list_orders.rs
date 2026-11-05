use crate::cli::Context;
use crate::parser::common::{print_key_value, print_section_header};
use crate::parser::orders::print_orders_table;
use crate::util::{fetch_events_list, ListKind};
use anyhow::{anyhow, Result};
use mostro_core::prelude::*;
use std::str::FromStr;

pub fn parse_status_filter(status: Option<&str>) -> Result<Status> {
    match status {
        Some(value) => Status::from_str(value)
            .map_err(|error| anyhow!("Not valid status '{}': {:?}", value, error)),
        None => Ok(Status::Pending),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_list_orders(
    kind: &Option<String>,
    currency: &Option<String>,
    status: &Option<String>,
    ctx: &Context,
) -> Result<()> {
    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency: Option<String> = None;
    // Default status is pending
    let status_checked = Some(parse_status_filter(status.as_deref())?);
    // Default kind is none
    let mut kind_checked: Option<mostro_core::order::Kind> = None;

    print_section_header("📋 List Orders");

    // Print status requested
    if let Some(status) = &status_checked {
        print_key_value("📊", "Status Filter", &format!("{:?}", status));
    }
    // Parse and validate kind from CLI string
    if let Some(k) = kind {
        kind_checked = Some(
            mostro_core::order::Kind::from_str(k)
                .map_err(|e| anyhow::anyhow!("Not valid order kind '{}': {:?}", k, e))?,
        );
        if let Some(kind) = &kind_checked {
            print_key_value("📈", "Order Type", &format!("{} orders", kind));
        }
    }

    // Uppercase currency
    if let Some(curr) = currency {
        upper_currency = Some(curr.to_uppercase());
        if let Some(currency) = &upper_currency {
            print_key_value("💱", "Currency Filter", currency);
        }
    }

    print_key_value("🎯", "Mostro PubKey", &ctx.mostro_pubkey.to_string());
    print_key_value("💡", "Action", "Fetching orders from relays...");
    println!();

    // Get orders from relays
    let table_of_orders = fetch_events_list(
        ListKind::Orders,
        status_checked,
        upper_currency,
        kind_checked,
        ctx,
        None,
    )
    .await?;
    let table = print_orders_table(table_of_orders)?;
    println!("{table}");

    Ok(())
}
