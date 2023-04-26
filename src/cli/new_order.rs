use mostro_core::{Action, Content, Kind, Status};
use nostr_sdk::prelude::ToBech32;

use std::collections::HashMap;
use std::io::{stdin, stdout, BufRead, Write};
use std::process;

use anyhow::Result;

use mostro_core::order::NewOrder;
use mostro_core::Message as MostroMessage;

use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};

use crate::pretty_table::print_order_preview;
use crate::util::{get_keys, send_order_id_cmd};

pub type FiatNames = HashMap<String, String>;

#[allow(clippy::too_many_arguments)]
pub async fn execute_new_order(
    kind: &Kind,
    fiat_code: &str,
    fiat_amount: &i64,
    amount: &i64,
    payment_method: &String,
    premium: &i64,
    invoice: &Option<String>,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    // Uppercase currency
    let fiat_code = fiat_code.to_uppercase();
    // Check if fiat currency selected is available on Yadio and eventually force user to set amount
    // this is in the case of crypto <--> crypto offer for example
    if *amount == 0 {
        // Get Fiat list
        let api_req_string = "https://api.yadio.io/currencies".to_string();
        let fiat_list_check = reqwest::get(api_req_string)
            .await?
            .json::<FiatNames>()
            .await?
            .contains_key(&fiat_code);
        if !fiat_list_check {
            println!("{} is not present in the fiat market, please specify an amount with -a flag to fix the rate", fiat_code);
            process::exit(0);
        }
    }
    let mut master_buyer_pubkey: Option<String> = None;
    let mut master_seller_pubkey: Option<String> = None;
    if kind == &Kind::Buy {
        master_buyer_pubkey = Some(my_key.public_key().to_bech32()?);
    } else {
        master_seller_pubkey = Some(my_key.public_key().to_bech32()?);
    }

    // Create new order for mostro
    let order_content = Content::Order(NewOrder::new(
        None,
        *kind,
        Status::Pending,
        *amount,
        fiat_code,
        *fiat_amount,
        payment_method.to_owned(),
        *premium,
        master_buyer_pubkey,
        master_seller_pubkey,
        invoice.as_ref().to_owned().cloned(),
        None,
    ));

    // Print order preview
    let ord_preview = print_order_preview(order_content.clone()).unwrap();
    println!("{ord_preview}");
    let mut user_input = String::new();
    let _input = stdin();
    print!("Check your order! Is it correct? (Y/n) > ");
    stdout().flush()?;

    let mut answer = stdin().lock();
    answer.read_line(&mut user_input)?;

    match user_input.to_lowercase().as_str().trim_end() {
        "y" | "" => {}
        "n" => {
            println!("Ok you have cancelled the order, create another one please");
            process::exit(0);
        }
        &_ => {
            println!("Can't get what you're sayin!");
            process::exit(0);
        }
    };
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    // Create fiat sent message
    let message = MostroMessage::new(
        0,
        None,
        Some(master_pubkey),
        Action::Order,
        Some(order_content),
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, message, false).await?;
    Ok(())
}
