use std::collections::HashSet;

use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use mostro_core::prelude::*;
use nip44::v2::{decrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    db::{Order, User},
    util::save_order,
};
use sqlx::SqlitePool;

/// Execute logic of command answer
pub async fn print_commands_results(message: &MessageKind, ctx: &Context) -> Result<()> {
    // Do the logic for the message response
    match message.action {
        Action::NewOrder => {
            if let Some(Payload::Order(order)) = message.payload.as_ref() {
                if let Some(req_id) = message.request_id {
                    if let Err(e) = save_order(
                        order.clone(),
                        &ctx.trade_keys,
                        req_id,
                        ctx.trade_index,
                        &ctx.pool,
                    )
                    .await
                    {
                        return Err(anyhow::anyhow!("Failed to save order: {}", e));
                    }

                    println!("🆕 New Order Created");
                    println!("═══════════════════════════════════════");
                    if let Some(order_id) = order.id {
                        println!("📋 Order ID: {}", order_id);
                    }
                    println!("💰 Amount: {} sats", order.amount);
                    println!("💱 Fiat Code: {}", order.fiat_code);
                    println!("💵 Fiat Amount: {}", order.fiat_amount);
                    println!("📊 Premium: {}%", order.premium);
                    println!("💳 Payment Method: {}", order.payment_method);
                    println!(
                        "📈 Kind: {:?}",
                        order
                            .kind
                            .as_ref()
                            .unwrap_or(&mostro_core::order::Kind::Sell)
                    );
                    println!(
                        "📊 Status: {:?}",
                        order.status.as_ref().unwrap_or(&Status::Pending)
                    );
                    println!("✅ Order saved successfully!");
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("No request id found in message"))
                }
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer adds an invoice to a takesell order
        Action::WaitingSellerToPay => {
            println!("⏳ Waiting for Seller Payment");
            println!("═══════════════════════════════════════");
            if let Some(order_id) = &message.id {
                println!("📋 Order ID: {}", order_id);
                let mut order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;
                match order
                    .set_status(Status::WaitingPayment.to_string())
                    .save(&ctx.pool)
                    .await
                {
                    Ok(_) => {
                        println!("📊 Status: Waiting for Payment");
                        println!("💡 The seller needs to pay the invoice to continue");
                        println!("✅ Order status updated successfully!");
                    }
                    Err(e) => println!("❌ Failed to update order status: {}", e),
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer adds an invoice to a takesell order
        Action::AddInvoice => {
            if let Some(Payload::Order(order)) = &message.payload {
                println!("⚡ Add Lightning Invoice");
                println!("═══════════════════════════════════════");
                if let Some(order_id) = order.id {
                    println!("📋 Order ID: {}", order_id);
                }
                println!("💰 Required Amount: {} sats", order.amount);
                println!("💡 Please add a lightning invoice with the exact amount above");
                println!();

                if let Some(req_id) = message.request_id {
                    // Save the order
                    if let Err(e) = save_order(
                        order.clone(),
                        &ctx.trade_keys,
                        req_id,
                        ctx.trade_index,
                        &ctx.pool,
                    )
                    .await
                    {
                        return Err(anyhow::anyhow!("Failed to save order: {}", e));
                    }
                    println!("✅ Order saved successfully!");
                } else {
                    return Err(anyhow::anyhow!("No request id found in message"));
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer pays the invoice coming from a takebuy
        Action::PayInvoice => {
            if let Some(Payload::PaymentRequest(order, invoice, _)) = &message.payload {
                println!("💳 Payment Invoice Received");
                println!("═══════════════════════════════════════");
                if let Some(order) = order {
                    if let Some(order_id) = order.id {
                        println!("📋 Order ID: {}", order_id);
                    }
                    println!("💰 Amount: {} sats", order.amount);
                    println!("💱 Fiat Code: {}", order.fiat_code);
                    println!("💵 Fiat Amount: {}", order.fiat_amount);
                }
                println!();
                println!("⚡ LIGHTNING INVOICE TO PAY:");
                println!("─────────────────────────────────────");
                println!("{}", invoice);
                println!("─────────────────────────────────────");
                println!("💡 Pay this invoice to continue the trade");
                println!();

                if let Some(order) = order {
                    if let Some(req_id) = message.request_id {
                        let store_order = order.clone();
                        // Save the order
                        if let Err(e) = save_order(
                            store_order,
                            &ctx.trade_keys,
                            req_id,
                            ctx.trade_index,
                            &ctx.pool,
                        )
                        .await
                        {
                            println!("❌ Failed to save order: {}", e);
                            return Err(anyhow::anyhow!("Failed to save order: {}", e));
                        }
                        println!("✅ Order saved successfully!");
                    } else {
                        return Err(anyhow::anyhow!("No request id found in message"));
                    }
                } else {
                    return Err(anyhow::anyhow!("No request id found in message"));
                }
            }
            Ok(())
        }
        Action::CantDo => {
            println!("❌ Action Cannot Be Completed");
            println!("═══════════════════════════════════════");
            match message.payload {
                Some(Payload::CantDo(Some(
                    CantDoReason::OutOfRangeFiatAmount | CantDoReason::OutOfRangeSatsAmount,
                ))) => {
                    println!("💰 Amount Error");
                    println!("💡 The amount is outside the allowed range");
                    println!("📊 Please check the order's min/max limits");
                    Err(anyhow::anyhow!(
                        "Amount is outside the allowed range. Please check the order's min/max limits."
                    ))
                }
                Some(Payload::CantDo(Some(CantDoReason::PendingOrderExists))) => {
                    println!("⏳ Pending Order Exists");
                    println!("💡 A pending order already exists");
                    println!("📊 Please wait for it to be filled or canceled");
                    Err(anyhow::anyhow!(
                        "A pending order already exists. Please wait for it to be filled or canceled."
                    ))
                }
                Some(Payload::CantDo(Some(CantDoReason::InvalidTradeIndex))) => {
                    println!("🔢 Invalid Trade Index");
                    println!("💡 The trade index is invalid");
                    println!("📊 Please synchronize the trade index with mostro");
                    Err(anyhow::anyhow!(
                        "Invalid trade index. Please synchronize the trade index with mostro"
                    ))
                }
                Some(Payload::CantDo(Some(CantDoReason::InvalidFiatCurrency))) => {
                    println!("💱 Invalid Currency");
                    println!("💡 The fiat currency is not supported");
                    println!("📊 Please use a valid currency");
                    Err(anyhow::anyhow!("Invalid currency"))
                }
                _ => {
                    println!("❓ Unknown Error");
                    println!("💡 An unknown error occurred");
                    Err(anyhow::anyhow!("Unknown reason: {:?}", message.payload))
                }
            }
        }
        // this is the case where the user cancels the order
        Action::Canceled => {
            if let Some(order_id) = &message.id {
                println!("🚫 Order Canceled");
                println!("═══════════════════════════════════════");
                println!("📋 Order ID: {}", order_id);

                // Acquire database connection
                // Verify order exists before deletion
                if Order::get_by_id(&ctx.pool, &order_id.to_string())
                    .await
                    .is_ok()
                {
                    if let Err(e) = Order::delete_by_id(&ctx.pool, &order_id.to_string()).await {
                        println!("❌ Failed to delete order: {}", e);
                        return Err(anyhow::anyhow!("Failed to delete order: {}", e));
                    }
                    // Release database connection
                    println!("✅ Order {} canceled successfully!", order_id);
                    Ok(())
                } else {
                    println!("❌ Order not found: {}", order_id);
                    Err(anyhow::anyhow!("Order not found: {}", order_id))
                }
            } else {
                Err(anyhow::anyhow!("No order id found in message"))
            }
        }
        Action::RateReceived => {
            println!("⭐ Rating Received");
            println!("═══════════════════════════════════════");
            println!("🙏 Thank you for your rating!");
            println!("💡 Your feedback helps improve the trading experience");
            println!("✅ Rating processed successfully!");
            Ok(())
        }
        Action::FiatSentOk => {
            if let Some(order_id) = &message.id {
                println!("💸 Fiat Payment Confirmed");
                println!("═══════════════════════════════════════");
                println!("📋 Order ID: {}", order_id);
                println!("✅ Fiat payment confirmation received");
                println!("⏳ Waiting for sats release from seller");
                println!("💡 The seller will now release your Bitcoin");
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order id found in message"))
            }
        }
        Action::LastTradeIndex => {
            if let Some(last_trade_index) = message.trade_index {
                println!("🔢 Last Trade Index Updated");
                println!("═══════════════════════════════════════");
                println!("📊 Last Trade Index: {}", last_trade_index);
                match User::get(&ctx.pool).await {
                    Ok(mut user) => {
                        user.set_last_trade_index(last_trade_index);
                        if let Err(e) = user.save(&ctx.pool).await {
                            println!("❌ Failed to update user: {}", e);
                        } else {
                            println!("✅ Trade index synchronized successfully!");
                        }
                    }
                    Err(_) => return Err(anyhow::anyhow!("Failed to get user")),
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No trade index found in message"))
            }
        }
        Action::DisputeInitiatedByYou => {
            if let Some(Payload::Dispute(dispute_id, _)) = &message.payload {
                println!("⚖️  Dispute Initiated");
                println!("═══════════════════════════════════════");
                println!("🆔 Dispute ID: {}", dispute_id);
                if let Some(order_id) = &message.id {
                    println!("📋 Order ID: {}", order_id);
                    let mut order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;
                    // Update order status to disputed if we have the order
                    match order
                        .set_status(Status::Dispute.to_string())
                        .save(&ctx.pool)
                        .await
                    {
                        Ok(_) => {
                            println!("📊 Status: Dispute");
                            println!("✅ Order status updated to Dispute");
                        }
                        Err(e) => println!("❌ Failed to update order status: {}", e),
                    }
                }
                println!("💡 A dispute has been initiated for this order");
                println!("✅ Dispute created successfully!");
                Ok(())
            } else {
                println!(
                    "⚠️  Warning: Dispute initiated but received unexpected payload structure"
                );
                Ok(())
            }
        }
        Action::HoldInvoicePaymentSettled | Action::Released => {
            println!("🎉 Payment Settled & Released");
            println!("═══════════════════════════════════════");
            println!("✅ Hold invoice payment settled successfully!");
            println!("💰 Bitcoin has been released to the buyer");
            println!("🎊 Trade completed successfully!");
            Ok(())
        }
        Action::Orders => {
            if let Some(Payload::Orders(orders)) = &message.payload {
                if orders.is_empty() {
                    println!("📋 Orders List");
                    println!("═══════════════════════════════════════");
                    println!("📭 No orders found or unauthorized access");
                } else {
                    println!("📋 Orders List");
                    println!("═══════════════════════════════════════");
                    println!("📊 Found {} order(s):", orders.len());
                    println!();
                    for (i, order) in orders.iter().enumerate() {
                        println!("📄 Order {}:", i + 1);
                        println!("─────────────────────────────────────");
                        println!(
                            "🆔 ID: {}",
                            order
                                .id
                                .as_ref()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "N/A".to_string())
                        );
                        println!(
                            "📈 Kind: {:?}",
                            order
                                .kind
                                .as_ref()
                                .unwrap_or(&mostro_core::order::Kind::Sell)
                        );
                        println!(
                            "📊 Status: {:?}",
                            order.status.as_ref().unwrap_or(&Status::Pending)
                        );
                        println!("💰 Amount: {} sats", order.amount);
                        println!("💱 Fiat Code: {}", order.fiat_code);
                        if let Some(min) = order.min_amount {
                            if let Some(max) = order.max_amount {
                                println!("💵 Fiat Range: {}-{}", min, max);
                            } else {
                                println!("💵 Fiat Amount: {}", order.fiat_amount);
                            }
                        } else {
                            println!("💵 Fiat Amount: {}", order.fiat_amount);
                        }
                        println!("💳 Payment Method: {}", order.payment_method);
                        println!("📊 Premium: {}%", order.premium);
                        if let Some(created_at) = order.created_at {
                            if let Some(expires_at) = order.expires_at {
                                println!(
                                    "📅 Created: {}",
                                    chrono::DateTime::from_timestamp(created_at, 0)
                                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                                );
                                println!(
                                    "⏰ Expires: {}",
                                    chrono::DateTime::from_timestamp(expires_at, 0)
                                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                                );
                            }
                        }
                        println!();
                    }
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No orders payload found in message"))
            }
        }
        Action::RestoreSession => {
            if let Some(Payload::RestoreData(restore_data)) = &message.payload {
                println!("🔄 Restore Session Response");
                println!("═══════════════════════════════════════");
                println!();

                // Process orders
                if !restore_data.restore_orders.is_empty() {
                    println!(
                        "📋 Found {} pending order(s):",
                        restore_data.restore_orders.len()
                    );
                    println!("─────────────────────────────────────");
                    for (i, order_info) in restore_data.restore_orders.iter().enumerate() {
                        println!("  {}. Order ID: {}", i + 1, order_info.order_id);
                        println!("     Trade Index: {}", order_info.trade_index);
                        println!("     Status: {:?}", order_info.status);
                        println!();
                    }
                } else {
                    println!("📋 No pending orders found.");
                    println!();
                }

                // Process disputes
                if !restore_data.restore_disputes.is_empty() {
                    println!(
                        "⚖️  Found {} active dispute(s):",
                        restore_data.restore_disputes.len()
                    );
                    println!("─────────────────────────────────────");
                    for (i, dispute_info) in restore_data.restore_disputes.iter().enumerate() {
                        println!("  {}. Dispute ID: {}", i + 1, dispute_info.dispute_id);
                        println!("     Order ID: {}", dispute_info.order_id);
                        println!("     Trade Index: {}", dispute_info.trade_index);
                        println!("     Status: {:?}", dispute_info.status);
                        println!();
                    }
                } else {
                    println!("⚖️  No active disputes found.");
                    println!();
                }

                println!("✅ Session restore completed successfully!");
                Ok(())
            } else {
                Err(anyhow::anyhow!("No restore data payload found in message"))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown action: {:?}", message.action)),
    }
}

pub async fn parse_dm_events(
    events: Events,
    pubkey: &Keys,
    since: Option<&i64>,
) -> Vec<(Message, u64, PublicKey)> {
    let mut id_set = HashSet::<EventId>::new();
    let mut direct_messages: Vec<(Message, u64, PublicKey)> = Vec::new();

    for dm in events.iter() {
        // Skip if already processed
        if !id_set.insert(dm.id) {
            continue;
        }

        let (created_at, message) = match dm.kind {
            nostr_sdk::Kind::GiftWrap => {
                let unwrapped_gift = match nip59::extract_rumor(pubkey, dm).await {
                    Ok(u) => u,
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not decrypt gift wrap (event {}): {}",
                            dm.id, e
                        );
                        continue;
                    }
                };
                let (message, _): (Message, Option<String>) =
                    match serde_json::from_str(&unwrapped_gift.rumor.content) {
                        Ok(msg) => msg,
                        Err(e) => {
                            eprintln!(
                                "Warning: Could not parse message content (event {}): {}",
                                dm.id, e
                            );
                            continue;
                        }
                    };
                (unwrapped_gift.rumor.created_at, message)
            }
            nostr_sdk::Kind::PrivateDirectMessage => {
                let ck = if let Ok(ck) = ConversationKey::derive(pubkey.secret_key(), &dm.pubkey) {
                    ck
                } else {
                    continue;
                };
                let b64decoded_content =
                    match general_purpose::STANDARD.decode(dm.content.as_bytes()) {
                        Ok(b64decoded_content) => b64decoded_content,
                        Err(_) => {
                            continue;
                        }
                    };
                let unencrypted_content = match decrypt_to_bytes(&ck, &b64decoded_content) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        continue;
                    }
                };
                let message_str = match String::from_utf8(unencrypted_content) {
                    Ok(s) => s,
                    Err(_) => {
                        continue;
                    }
                };
                let message = match Message::from_json(&message_str) {
                    Ok(m) => m,
                    Err(_) => {
                        continue;
                    }
                };
                (dm.created_at, message)
            }
            _ => continue,
        };
        // check if the message is older than the since time if it is, skip it
        if let Some(since_time) = since {
            // Calculate since time from now in minutes subtracting the since time
            let since_time = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::minutes(*since_time))
                .unwrap()
                .timestamp() as u64;

            if created_at.as_u64() < since_time {
                continue;
            }
        }
        direct_messages.push((message, created_at.as_u64(), dm.pubkey));
    }
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));
    direct_messages
}

pub async fn print_direct_messages(
    dm: &[(Message, u64, PublicKey)],
    _pool: &SqlitePool,
    mostro_pubkey: Option<PublicKey>,
) -> Result<()> {
    if dm.is_empty() {
        println!();
        println!("📭 No new messages");
        println!();
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(160)
        .set_header(vec![
            Cell::new("⏰ Time")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("📨 From")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("🎯 Action")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("📝 Details")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);

    let mut rows: Vec<Row> = Vec::new();
    for (message, created_at, sender_pubkey) in dm.iter() {
        let date = match DateTime::from_timestamp(*created_at as i64, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "Invalid timestamp".to_string(),
        };

        let inner = message.get_inner_message_kind();
        let action_str = inner.action.to_string();

        // Select an icon for the action/payload
        let action_icon = match inner.action {
            Action::NewOrder => "🆕",
            Action::AddInvoice | Action::PayInvoice => "⚡",
            Action::FiatSent | Action::FiatSentOk => "💸",
            Action::Release | Action::Released => "🔓",
            Action::Cancel | Action::Canceled => "🚫",
            Action::Dispute | Action::DisputeInitiatedByYou => "⚖️",
            Action::RateUser | Action::RateReceived => "⭐",
            Action::Orders => "📋",
            Action::LastTradeIndex => "🔢",
            Action::SendDm => "💬",
            _ => "🎯",
        };

        let mut action_cell = Cell::new(format!("{} {}", action_icon, action_str))
            .set_alignment(CellAlignment::Center);
        let action_lower = action_str.to_lowercase();
        if action_lower.contains("invoice") {
            action_cell = action_cell.fg(Color::Cyan);
        } else if action_lower.contains("dispute") {
            action_cell = action_cell.fg(Color::Red);
        } else if action_lower.contains("rate") || action_lower.contains("released") {
            action_cell = action_cell.fg(Color::Green);
        } else if action_lower.contains("cancel") {
            action_cell = action_cell.fg(Color::Red);
        }

        // Build details summary
        let details = if let Some(payload) = &inner.payload {
            match payload {
                Payload::TextMessage(t) => format!("✉️ {}", t),
                Payload::PaymentRequest(_, inv, _) => format!("⚡ Invoice: {}", inv),
                Payload::Dispute(id, _) => format!("⚖️ Dispute ID: {}", id),
                Payload::Order(o) if inner.action == Action::NewOrder => format!(
                    "🆕 Order: {} {} sats ({})",
                    o.id.as_ref()
                        .map(|x| x.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    o.amount,
                    o.fiat_code
                ),
                _ => format!("{:?}", payload),
            }
        } else {
            "-".to_string()
        };

        // Truncate long details for compact table row
        let details = if details.len() > 120 {
            format!("{}…", &details[..120])
        } else {
            details
        };

        // From cell: show 🧌 Mostro if matches provided pubkey
        let from_label = if let Some(pk) = mostro_pubkey {
            if *sender_pubkey == pk {
                format!("🧌 {}", sender_pubkey.to_hex())
            } else {
                sender_pubkey.to_hex()
            }
        } else {
            sender_pubkey.to_hex()
        };

        let row = Row::from(vec![
            Cell::new(date).set_alignment(CellAlignment::Center),
            Cell::new(from_label).set_alignment(CellAlignment::Center),
            action_cell,
            Cell::new(details),
        ]);
        rows.push(row);
    }

    table.add_rows(rows);
    println!("{table}");
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {}
