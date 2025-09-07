use crate::parser::orders::print_orders_table;
use crate::util::{fetch_events_list, ListKind};
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::str::FromStr;

pub struct ListOrdersParams<'a> {
    pub kind: &'a Option<String>,
    pub currency: &'a Option<String>,
    pub status: &'a Option<String>,
    pub mostro_pubkey: PublicKey,
    pub mostro_keys: &'a Keys,
    pub trade_index: i64,
    pub pool: &'a SqlitePool,
    pub client: &'a Client,
}

pub async fn execute_list_orders(params: ListOrdersParams<'_>) -> Result<()> {
    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency: Option<String> = None;
    let mut status_checked: Option<Status> = Some(Status::from_str("pending").unwrap());
    let mut kind_checked: Option<mostro_core::order::Kind> = None;

    // New check against strings
    if let Some(s) = params.status {
        status_checked = Some(Status::from_str(s).expect("Not valid status! Please check"));
    }

    println!(
        "You are searching orders with status {:?}",
        status_checked.unwrap()
    );
    // New check against strings
    if let Some(k) = params.kind {
        kind_checked = Some(
            mostro_core::order::Kind::from_str(k).expect("Not valid order kind! Please check"),
        );
        println!("You are searching {} orders", kind_checked.unwrap());
    }

    // Uppercase currency
    if let Some(curr) = params.currency {
        upper_currency = Some(curr.to_uppercase());
        println!(
            "You are searching orders with currency {}",
            upper_currency.clone().unwrap()
        );
    }

    println!(
        "Requesting orders from mostro pubId - {}",
        params.mostro_pubkey
    );

    // Get orders from relays
    let table_of_orders = fetch_events_list(crate::util::FetchEventsParams {
        list_kind: ListKind::Orders,
        status: status_checked,
        currency: upper_currency,
        kind: kind_checked,
        mostro_pubkey: params.mostro_pubkey,
        mostro_keys: params.mostro_keys,
        trade_index: params.trade_index,
        pool: params.pool,
        client: params.client,
    })
    .await?;
    let table = print_orders_table(table_of_orders)?;
    println!("{table}");

    Ok(())
}
