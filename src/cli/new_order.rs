use crate::cli::Context;
use crate::parser::orders::print_order_preview;
use crate::util::{send_dm, uppercase_first, wait_for_dm};
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::io::{stdin, stdout, BufRead, Write};
use std::process;
use std::str::FromStr;
use uuid::Uuid;

pub type FiatNames = HashMap<String, String>;

fn set_order_values() -> Result<SmallOrder> {
    let mut new_order= SmallOrder::default();

    ;
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_new_order(
    kind: &str,
    fiat_code: &str,
    fiat_amount: &(i64, Option<i64>),
    amount: &i64,
    payment_method: &str,
    premium: &i64,
    invoice: &Option<String>,
    ctx: &Context,
    expiration_days: &i64,
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
    let kind = uppercase_first(kind);
    // New check against strings
    let kind_checked = mostro_core::order::Kind::from_str(&kind)
        .map_err(|_| anyhow::anyhow!("Invalid order kind"))?;
    let expires_at = match *expiration_days {
        0 => None,
        _ => {
            let now = chrono::Utc::now();
            let expires_at = now + chrono::Duration::days(*expiration_days);
            Some(expires_at.timestamp())
        }
    };

    // Get the type of neworder
    // if both tuple field are valid than it's a range order
    // otherwise use just fiat amount value as before
    let amt = if fiat_amount.1.is_some() {
        (0, Some(fiat_amount.0), fiat_amount.1)
    } else {
        (fiat_amount.0, None, None)
    };

    let small_order = set_order_values(kind, fiat_code, amt, payment_method, premium, invoice, expires_at)?;
    
    let small_order = SmallOrder::new(
        None,
        Some(kind_checked),
        Some(Status::Pending),
        *amount,
        fiat_code.clone(),
        amt.1,
        amt.2,
        amt.0,
        payment_method.to_owned(),
        *premium,
        None,
        None,
        invoice.as_ref().to_owned().cloned(),
        Some(0),
        expires_at,
    );

    // Create new order for mostro
    let order_content = Payload::Order(small_order.clone());

    // Print order preview
    let ord_preview = print_order_preview(order_content.clone())
        .map_err(|e| anyhow::anyhow!("Failed to generate order preview: {}", e))?;
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
    let request_id = Uuid::new_v4().as_u128() as u64;
    // Create NewOrder message
    let message = Message::new_order(
        None,
        Some(request_id),
        Some(ctx.trade_index),
        Action::NewOrder,
        Some(order_content),
    );

    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade keys: {:?}",
        ctx.trade_keys.public_key().to_hex()
    );

    // Serialize the message
    let message_json = message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Clone the keys and client for the async call
    let identity_keys_clone = ctx.identity_keys.clone();
    let trade_keys_clone = ctx.trade_keys.clone();
    let client_clone = ctx.client.clone();
    let mostro_pubkey_clone = ctx.mostro_pubkey;

    // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
    let subscription = Filter::new()
        .pubkey(ctx.trade_keys.public_key())
        .kind(nostr_sdk::Kind::GiftWrap)
        .limit(0);

    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEvents(1));

    ctx.client.subscribe(subscription, Some(opts)).await?;

    // Spawn a new task to send the DM
    // This is so we can wait for the gift wrap event in the main thread
    tokio::spawn(async move {
        let _ = send_dm(
            &client_clone,
            Some(&identity_keys_clone),
            &trade_keys_clone,
            &mostro_pubkey_clone,
            message_json,
            None,
            false,
        )
        .await;
    });

    // Wait for the DM to be sent from mostro
    wait_for_dm(
        &ctx.client,
        &ctx.trade_keys,
        request_id,
        Some(ctx.trade_index),
        None,
        &ctx.pool,
    )
    .await?;

    Ok(())
}
