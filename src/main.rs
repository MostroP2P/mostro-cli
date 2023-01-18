use clap::{Parser, Subcommand};
use dotenvy::{dotenv, var};
use nostr::util::nips::nip19::FromBech32;
use nostr::util::time::timestamp;
use nostr::{Kind, SubscriptionFilter};
use nostr_sdk::{RelayPoolNotifications, Result};
use std::env::set_var;

pub mod types;
pub mod util;
use crate::util::{get_orders_list, print_orders_table};

/// cli arguments
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
/// Mostro P2P cli client
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Requests open orders from mostro pubkey ()
    Listorders {
        pubkey: String,
        #[clap(default_value = "Pending")]
        orderstatus: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();
    // TODO: handle arguments
    let cli = Cli::parse();
    //Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
    }

    // mostro pubkey
    let pubkey = var("MOSTRO_PUBKEY").expect("$MOSTRO_PUBKEY env var needs to be set");
    let mostro_keys = nostr::key::XOnlyPublicKey::from_bech32(pubkey)?;

    //Call function to connect to relays
    let client = crate::util::connect_nostr().await?;

    let subscription = SubscriptionFilter::new()
        .author(mostro_keys)
        .since(timestamp());

    client.subscribe(vec![subscription]).await?;

    match &cli.command {
        Some(Commands::Listorders {
            pubkey,
            orderstatus,
        }) => {
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

    // Handle notifications
    loop {
        let mut notifications = client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotifications::ReceivedEvent(event) = notification {
                if let Kind::Custom(kind) = event.kind {
                    if (30000..40000).contains(&kind) {
                        let order = types::Order::from_json(&event.content)?;
                        println!("Event id: {}", event.id);
                        println!("Event kind: {}", kind);
                        println!("Order: {:#?}", order);
                    }
                }
            }
        }
    }
}
