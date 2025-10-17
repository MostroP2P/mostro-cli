use crate::cli::Context;
use crate::parser::orders::print_order_preview;
use crate::util::{print_dm_events, send_dm, uppercase_first, wait_for_dm};
use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use mostro_core::prelude::*;
use std::collections::HashMap;
use std::io::{stdin, stdout, BufRead, Write};
use std::process;
use std::str::FromStr;
use uuid::Uuid;

pub type FiatNames = HashMap<String, String>;

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
    println!("🆕 Create New Order");
    println!("═══════════════════════════════════════");

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);

    let mut rows: Vec<Row> = Vec::new();
    rows.push(Row::from(vec![Cell::new("📈 Order Type"), Cell::new(kind)]));
    rows.push(Row::from(vec![
        Cell::new("💱 Fiat Code"),
        Cell::new(fiat_code),
    ]));
    rows.push(Row::from(vec![
        Cell::new("💰 Amount (sats)"),
        Cell::new(amount.to_string()),
    ]));
    if let Some(max) = fiat_amount.1 {
        rows.push(Row::from(vec![
            Cell::new("📊 Fiat Range"),
            Cell::new(format!("{}-{}", fiat_amount.0, max)),
        ]));
    } else {
        rows.push(Row::from(vec![
            Cell::new("💵 Fiat Amount"),
            Cell::new(fiat_amount.0.to_string()),
        ]));
    }
    rows.push(Row::from(vec![
        Cell::new("💳 Payment Method"),
        Cell::new(payment_method),
    ]));
    rows.push(Row::from(vec![
        Cell::new("📊 Premium (%)"),
        Cell::new(premium.to_string()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("🔢 Trade Index"),
        Cell::new(ctx.trade_index.to_string()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("🔑 Trade Keys"),
        Cell::new(ctx.trade_keys.public_key().to_hex()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("🎯 Target"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));

    table.add_rows(rows);
    println!("{table}");
    println!("💡 Sending new order to Mostro...\n");

    // Serialize the message
    let message_json = message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the DM
    let sent_message = send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Wait for the DM to be sent from mostro
    let recv_event = wait_for_dm(ctx, None, sent_message).await?;

    // Parse the incoming DM
    print_dm_events(recv_event, request_id, ctx, None).await?;

    Ok(())
}
