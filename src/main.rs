use clap::Parser;
use dotenvy::{dotenv, var};
use nostr_sdk::prelude::*;
use std::env::set_var;

pub mod cli;
pub mod types;
pub mod util;
pub mod fiat;
use crate::util::{get_orders_list, print_orders_table};
use crate::fiat::{check_currency_ticker};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();
    // TODO: handle arguments
    let cli = cli::Cli::parse();
    //Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
    }

    // mostro pubkey
    let pubkey = var("MOSTRO_PUBKEY").expect("$MOSTRO_PUBKEY env var needs to be set");
    //Used to get upper currency string to check against a list of tickers
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

            //Validate currency ticker
            match currency  {  
                Some(cur)  => {   
                    upper_currency = check_currency_ticker(cur.clone());
                    if upper_currency.is_none(){
                        println!("The currency ticker {} you have selected is not available, use a valid one!", cur.clone());
                        std::process::exit(0)
                    }
                },
                None => println!("You have selected offers of all supported currencies") 
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
            let tableoforders = get_orders_list(
                mostro_key,
                order_status.to_owned(),
                upper_currency.clone(),
                *kind_order,
                &client,
            )
            .await?;
            let table = print_orders_table(tableoforders)?;
            println!("{table}");
            std::process::exit(0);
        }
        None => {}
    }

    Ok(())
}
