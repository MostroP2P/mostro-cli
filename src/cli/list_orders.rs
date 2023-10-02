use mostro_core::order::{Kind, Status};

use anyhow::Result;

use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::Client;

use crate::pretty_table::print_orders_table;
use crate::util::get_orders_list;

pub async fn execute_list_orders(
    kind: &Option<Kind>,
    currency: &Option<String>,
    status: &Option<Status>,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency: Option<String> = None;

    // Uppercase currency
    if let Some(curr) = currency {
        upper_currency = Some(curr.to_uppercase());
    }

    println!(
        "Requesting orders from mostro pubId - {}",
        mostro_key.clone()
    );
    println!("You are searching {:?} orders", status.unwrap().clone());

    //Get orders from relays
    let table_of_orders = get_orders_list(
        mostro_key,
        status.to_owned(),
        upper_currency.clone(),
        *kind,
        client,
    )
    .await?;
    let table = print_orders_table(table_of_orders)?;
    println!("{table}");

    Ok(())
}
