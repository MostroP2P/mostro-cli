use anyhow::Result;
use mostro_core::order::{Kind, Status};
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::Client;
use std::str::FromStr;

use crate::pretty_table::print_orders_table;
use crate::util::get_orders_list;

pub async fn execute_list_orders(
    kind: &Option<String>,
    currency: &Option<String>,
    status: &Option<String>,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency: Option<String> = None;
    let mut status_checked: Option<Status> = Some(Status::from_str("Pending").unwrap());
    let mut kind_checked: Option<Kind> = None;

    // New check against strings
    if let Some(s) = status {
        status_checked = Some(Status::from_str(s).unwrap());
    }

    println!(
        "You are searching orders with status {:?}",
        status_checked.unwrap()
    );
    // New check against strings
    if let Some(k) = kind {
        kind_checked = Some(Kind::from_str(k).unwrap());
        println!("You are searching {:?} orders", kind_checked.clone());
    }

    // Uppercase currency
    if let Some(curr) = currency {
        upper_currency = Some(curr.to_uppercase());
        println!(
            "You are searching orders with currency {:?}",
            upper_currency.clone()
        );
    }

    println!(
        "Requesting orders from mostro pubId - {}",
        mostro_key.clone()
    );

    // Get orders from relays
    let table_of_orders = get_orders_list(
        mostro_key,
        status_checked,
        upper_currency.clone(),
        kind_checked,
        client,
    )
    .await?;
    let table = print_orders_table(table_of_orders)?;
    println!("{table}");

    Ok(())
}
