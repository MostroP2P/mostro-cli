use crate::cli::Context;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::parser::orders::print_order_preview;
use crate::util::{print_dm_events, send_dm, uppercase_first, wait_for_dm};
use anyhow::{anyhow, bail, Result};
use chrono::{Duration, Utc};
use mostro_core::order::Kind as OrderKind;
use mostro_core::prelude::*;
use reqwest::get;
use std::collections::HashMap;
use std::io::{stdin, stdout, BufRead, Write};
use std::str::FromStr;
use uuid::Uuid;

pub type FiatNames = HashMap<String, String>;

pub fn validate_market_fiat(fiat_code: &str, fiat_names: &FiatNames) -> Result<()> {
    if !fiat_names.contains_key(fiat_code) {
        bail!("{} is not present in the fiat market, please specify an amount with -a flag to fix the rate", fiat_code);
    }
    Ok(())
}

pub fn confirm_new_order(mut input: impl BufRead) -> Result<()> {
    let mut user_input = String::new();
    if input.read_line(&mut user_input)? == 0 {
        bail!("No order confirmation received: stdin reached EOF");
    }
    match user_input.to_lowercase().as_str().trim_end() {
        "y" | "" => Ok(()),
        "n" => bail!("Order cancelled"),
        _ => bail!("Invalid order confirmation; enter y or n"),
    }
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
        let fiat_names = get(api_req_string).await?.json::<FiatNames>().await?;
        validate_market_fiat(&fiat_code, &fiat_names)?;
    }
    let kind = uppercase_first(kind);
    // New check against strings
    let kind_checked = OrderKind::from_str(&kind).map_err(|_| anyhow!("Invalid order kind"))?;
    let expires_at = match *expiration_days {
        0 => None,
        _ => {
            let now = Utc::now();
            let expires_at = now + Duration::days(*expiration_days);
            Some(expires_at.timestamp())
        }
    };

    // Get the type of neworder
    // If both tuple fields are set, treat as a range order
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
        .map_err(|error| anyhow!("Failed to generate order preview: {}", error))?;
    println!("{ord_preview}");
    stdout().flush()?;
    confirm_new_order(stdin().lock())?;
    let request_id = Uuid::new_v4().as_u128() as u64;
    // Create NewOrder message
    let message = Message::new_order(
        None,
        Some(request_id),
        Some(ctx.trade_index),
        Action::NewOrder,
        Some(order_content),
    );

    // Print summary and send DM to Mostro
    println!("🆕 Create New Order");
    println!("═══════════════════════════════════════");

    let mut table = create_standard_table();
    table.set_header(create_field_value_header());

    table.add_row(create_emoji_field_row("📈 ", "Order Type", &kind));
    table.add_row(create_emoji_field_row("💱 ", "Fiat Code", &fiat_code));
    table.add_row(create_emoji_field_row(
        "💰 ",
        "Amount (sats)",
        &amount.to_string(),
    ));

    if let Some(max) = fiat_amount.1 {
        table.add_row(create_emoji_field_row(
            "📊 ",
            "Fiat Range",
            &format!("{}-{}", fiat_amount.0, max),
        ));
    } else {
        table.add_row(create_emoji_field_row(
            "💵 ",
            "Fiat Amount",
            &fiat_amount.0.to_string(),
        ));
    }

    table.add_row(create_emoji_field_row(
        "💳 ",
        "Payment Method",
        payment_method,
    ));
    table.add_row(create_emoji_field_row(
        "📈 ",
        "Premium (%)",
        &premium.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🔢 ",
        "Trade Index",
        &ctx.trade_index.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🔑 ",
        "Trade Key",
        &ctx.trade_keys.public_key().to_hex(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Target",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{}", table);
    println!("\n💡 Sending new order to Mostro...\n");

    // Serialize the message
    let message_json = message
        .as_json()
        .map_err(|_| anyhow!("Failed to serialize message"))?;

    // Send the DM
    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
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
