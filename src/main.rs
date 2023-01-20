use clap::Parser;
use dotenvy::{dotenv, var};
use nostr::util::nips::nip19::FromBech32;
use nostr_sdk::Result;
use std::env::set_var;

pub mod cli;
pub mod types;
pub mod util;
use crate::util::{get_orders_list, print_orders_table};

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

    // Call function to connect to relays
    let client = crate::util::connect_nostr().await?;

    match &cli.command {
        Some(cli::Commands::Listorders { orderstatus }) => {
            let mostro_key = nostr::key::XOnlyPublicKey::from_bech32(pubkey)?;

            println!(
                "Requesting orders from mostro pubId - {}",
                mostro_key.clone()
            );
            println!("You are searching {} orders", orderstatus.clone());

            //Get orders from relays
            let tableoforders =
                get_orders_list(mostro_key, orderstatus.to_owned(), &client).await?;
            let table = print_orders_table(tableoforders)?;
            println!("{}", table);
            std::process::exit(0);
        }
        None => {}
    }

    Ok(())
}
