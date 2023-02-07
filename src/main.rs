use clap::Parser;
use dotenvy::{dotenv, var};
use nostr_sdk::prelude::*;
use std::env::set_var;

pub mod cli;
pub mod types;
pub mod util;
pub mod lightning;
pub mod error;

use crate::util::{get_orders_list, print_orders_table, take_order_id};
use lightning::is_valid_invoice;

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
    
    // My key
    let my_key = crate::util::get_keys()?;

    // Used to get upper currency string to check against a list of tickers
    let mut upper_currency = None;

    // Call function to connect to relays
    let client = crate::util::connect_nostr().await?;

    // let mut ln_client = crate::lightning::LndConnector::new().await;

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
        Some(cli::Commands::Takesell { 
            order_id, 
            invoice 
        }) => {
            let mostro_key = XOnlyPublicKey::from_bech32(pubkey)?;

            println!(
                "Request of take order {} from mostro pubId {}",
                order_id,
                mostro_key.clone()
            );

            // Check invoice string
            let valid_invoice = is_valid_invoice(invoice);
            match valid_invoice{
                Ok(_) => {
                    take_order_id(&client, &my_key, mostro_key, order_id, invoice).await?;                
                    std::process::exit(0);
                },
                Err(e) => println!("{}",e) 
            }
        },
        None => {}
    }
    println!("Bye Bye!");
    Ok(())
}
