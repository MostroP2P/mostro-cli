use clap::Parser;
use dotenvy::{dotenv, var};
use nostr_sdk::prelude::*;
use std::env::set_var;

pub mod cli;
pub mod types;
pub mod util;
use crate::util::{get_orders_list, print_orders_table};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    // TODO: handle arguments
    let cli = cli::Cli::parse();
    //Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
    }

    pretty_env_logger::init();

    // Mostro pubkey
    let pubkey = var("MOSTRO_PUBKEY").expect("$MOSTRO_PUBKEY env var needs to be set");
    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency = None;

    // Call function to connect to relays
    let client = crate::util::connect_nostr().await?;

    match &cli.command {
        Some(cli::Commands::ListOrders {
            order_status,
            currency,
            kind_order,
        }) => {
            let mostro_key = XOnlyPublicKey::from_bech32(pubkey)?;

            // Uppercase currency
            if let Some(curr) = currency {
                upper_currency = Some(curr.to_uppercase());
            }

            println!(
                "Requesting orders from mostro pubId - {}",
                mostro_key.clone()
            );
            println!(
                "You are searching {:?} orders",
                order_status.unwrap().clone()
            );

            //Get orders from relays
            let table_of_orders = get_orders_list(
                mostro_key,
                order_status.to_owned(),
                upper_currency.clone(),
                *kind_order,
                &client,
            )
            .await?;
            let table = print_orders_table(table_of_orders)?;
            println!("{table}");
            std::process::exit(0);
        }
        None => {}
    }

    Ok(())
}
