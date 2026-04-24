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
    parser::common::{
        format_timestamp, print_amount_info, print_fiat_code, print_order_count,
        print_payment_method, print_premium, print_required_amount, print_section_header,
        print_success_message, print_trade_index,
    },
    util::save_order,
};
use serde_json;

/// Handle new order creation display
fn handle_new_order_display(order: &mostro_core::order::SmallOrder) {
    print_section_header("🆕 New Order Created");
    if let Some(order_id) = order.id {
        println!("📋 Order ID: {}", order_id);
    }
    print_amount_info(order.amount);
    print_fiat_code(&order.fiat_code);
    println!("💵 Fiat Amount: {}", order.fiat_amount);
    print_premium(order.premium);
    print_payment_method(&order.payment_method);
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
    print_success_message("Order saved successfully!");
}

/// Handle add invoice display
fn handle_add_invoice_display(order: &mostro_core::order::SmallOrder) {
    print_section_header("⚡ Add Lightning Invoice");
    if let Some(order_id) = order.id {
        println!("📋 Order ID: {}", order_id);
    }
    print_required_amount(order.amount);
    println!("💡 Please add a lightning invoice with the exact amount above");
    println!();
}

/// Handle pay invoice display
fn handle_pay_invoice_display(order: &Option<mostro_core::order::SmallOrder>, invoice: &str) {
    print_section_header("💳 Payment Invoice Received");
    if let Some(order) = order {
        if let Some(order_id) = order.id {
            println!("📋 Order ID: {}", order_id);
        }
        print_amount_info(order.amount);
        print_fiat_code(&order.fiat_code);
        println!("💵 Fiat Amount: {}", order.fiat_amount);
    }
    println!();
    println!("⚡ LIGHTNING INVOICE TO PAY:");
    println!("─────────────────────────────────────");
    println!("{}", invoice);
    println!("─────────────────────────────────────");
    println!("💡 Pay this invoice to continue the trade");
    println!();
}

/// Format payload details for DM table display
fn format_payload_details(payload: &Payload, action: &Action) -> String {
    match payload {
        Payload::TextMessage(t) => format!("✉️ {}", t),
        Payload::PaymentRequest(_, inv, _) => {
            // For invoices, show the full invoice without truncation
            format!("⚡ Lightning Invoice:\n{}", inv)
        }
        Payload::Dispute(id, _) => format!("⚖️ Dispute ID: {}", id),
        Payload::Order(o) if *action == Action::NewOrder => format!(
            "🆕 New Order: {} {} sats ({})",
            o.id.as_ref()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            o.amount,
            o.fiat_code
        ),
        Payload::Order(o) => {
            // Pretty format order details
            let status_emoji = match o.status.as_ref().unwrap_or(&Status::Pending) {
                Status::Pending => "⏳",
                Status::Active => "✅",
                Status::Dispute => "⚖️",
                Status::Canceled => "🚫",
                Status::CanceledByAdmin => "🚫",
                Status::CooperativelyCanceled => "🤝",
                Status::Success => "🎉",
                Status::FiatSent => "💸",
                Status::WaitingPayment => "⏳",
                Status::WaitingBuyerInvoice => "⚡",
                Status::SettledByAdmin => "✅",
                Status::CompletedByAdmin => "🎉",
                Status::Expired => "⏰",
                Status::SettledHoldInvoice => "💰",
                Status::InProgress => "🔄",
            };

            let kind_emoji = match o.kind.as_ref().unwrap_or(&mostro_core::order::Kind::Sell) {
                mostro_core::order::Kind::Buy => "📈",
                mostro_core::order::Kind::Sell => "📉",
            };

            format!(
                "📋 Order: {} {} sats ({})\n{} Status: {:?}\n{} Kind: {:?}",
                o.id.as_ref()
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                o.amount,
                o.fiat_code,
                status_emoji,
                o.status.as_ref().unwrap_or(&Status::Pending),
                kind_emoji,
                o.kind.as_ref().unwrap_or(&mostro_core::order::Kind::Sell)
            )
        }
        Payload::Peer(peer) => {
            // Pretty format peer information
            if let Some(reputation) = &peer.reputation {
                let rating_emoji = if reputation.rating >= 4.0 {
                    "⭐"
                } else if reputation.rating >= 3.0 {
                    "🔶"
                } else if reputation.rating >= 2.0 {
                    "🔸"
                } else {
                    "🔻"
                };

                format!(
                    "👤 Peer: {}\n{} Rating: {:.1}/5.0\n📊 Reviews: {}\n📅 Operating Days: {}",
                    if peer.pubkey.is_empty() {
                        "Anonymous"
                    } else {
                        &peer.pubkey
                    },
                    rating_emoji,
                    reputation.rating,
                    reputation.reviews,
                    reputation.operating_days
                )
            } else {
                format!(
                    "👤 Peer: {}",
                    if peer.pubkey.is_empty() {
                        "Anonymous"
                    } else {
                        &peer.pubkey
                    }
                )
            }
        }
        _ => {
            // For other payloads, try to pretty-print as JSON
            match serde_json::to_string_pretty(payload) {
                Ok(json) => format!("📄 Payload:\n{}", json),
                Err(_) => format!("📄 Payload: {:?}", payload),
            }
        }
    }
}

/// Handle orders list display
fn handle_orders_list_display(orders: &[mostro_core::order::SmallOrder]) {
    if orders.is_empty() {
        print_section_header("📋 Orders List");
        println!("📭 No orders found or unauthorized access");
    } else {
        print_section_header("📋 Orders List");
        print_order_count(orders.len());
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
            print_amount_info(order.amount);
            print_fiat_code(&order.fiat_code);
            if let Some(min) = order.min_amount {
                if let Some(max) = order.max_amount {
                    println!("💵 Fiat Range: {}-{}", min, max);
                } else {
                    println!("💵 Fiat Amount: {}", order.fiat_amount);
                }
            } else {
                println!("💵 Fiat Amount: {}", order.fiat_amount);
            }
            print_payment_method(&order.payment_method);
            print_premium(order.premium);
            if let Some(created_at) = order.created_at {
                if let Some(expires_at) = order.expires_at {
                    println!("📅 Created: {}", format_timestamp(created_at));
                    println!("⏰ Expires: {}", format_timestamp(expires_at));
                }
            }
            println!();
        }
    }
}

/// Display SolverDisputeInfo in a beautiful table format
fn display_solver_dispute_info(dispute_info: &mostro_core::dispute::SolverDisputeInfo) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(120)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);

    let mut rows: Vec<Row> = Vec::new();

    // Basic dispute information
    rows.push(Row::from(vec![
        Cell::new("📋 Order ID:"),
        Cell::new(dispute_info.id.to_string()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("📊 Kind"),
        Cell::new(dispute_info.kind.clone()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("📈 Status"),
        Cell::new(dispute_info.status.clone()),
    ]));

    // Financial information
    rows.push(Row::from(vec![
        Cell::new("💰 Amount"),
        Cell::new(format!("{} sats", dispute_info.amount)),
    ]));
    rows.push(Row::from(vec![
        Cell::new("💵 Fiat Amount"),
        Cell::new(dispute_info.fiat_amount.to_string()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("📊 Premium"),
        Cell::new(format!("{}%", dispute_info.premium)),
    ]));
    rows.push(Row::from(vec![
        Cell::new("💳 Payment Method"),
        Cell::new(dispute_info.payment_method.clone()),
    ]));
    rows.push(Row::from(vec![
        Cell::new("💸 Fee"),
        Cell::new(format!("{} sats", dispute_info.fee)),
    ]));
    rows.push(Row::from(vec![
        Cell::new("🛣️ Routing Fee"),
        Cell::new(format!("{} sats", dispute_info.routing_fee)),
    ]));

    // Participant information
    rows.push(Row::from(vec![
        Cell::new("👤 Initiator"),
        Cell::new(dispute_info.initiator_pubkey.clone()),
    ]));

    if let Some(buyer) = &dispute_info.buyer_pubkey {
        rows.push(Row::from(vec![
            Cell::new("🛒 Buyer"),
            Cell::new(buyer.clone()),
        ]));
    }

    if let Some(seller) = &dispute_info.seller_pubkey {
        rows.push(Row::from(vec![
            Cell::new("🏪 Seller"),
            Cell::new(seller.clone()),
        ]));
    }

    // Privacy settings
    rows.push(Row::from(vec![
        Cell::new("🔒 Initiator Privacy"),
        Cell::new(if dispute_info.initiator_full_privacy {
            "Full Privacy"
        } else {
            "Standard"
        }),
    ]));
    rows.push(Row::from(vec![
        Cell::new("🔒 Counterpart Privacy"),
        Cell::new(if dispute_info.counterpart_full_privacy {
            "Full Privacy"
        } else {
            "Standard"
        }),
    ]));

    // Optional fields
    if let Some(hash) = &dispute_info.hash {
        rows.push(Row::from(vec![
            Cell::new("🔐 Hash"),
            Cell::new(hash.clone()),
        ]));
    }

    if let Some(preimage) = &dispute_info.preimage {
        rows.push(Row::from(vec![
            Cell::new("🔑 Preimage"),
            Cell::new(preimage.clone()),
        ]));
    }

    if let Some(buyer_invoice) = &dispute_info.buyer_invoice {
        rows.push(Row::from(vec![
            Cell::new("⚡ Buyer Invoice"),
            Cell::new(buyer_invoice.clone()),
        ]));
    }

    // Status information
    rows.push(Row::from(vec![
        Cell::new("📊 Previous Status"),
        Cell::new(dispute_info.order_previous_status.clone()),
    ]));

    // Timestamps
    rows.push(Row::from(vec![
        Cell::new("📅 Created"),
        Cell::new(format_timestamp(dispute_info.created_at)),
    ]));
    rows.push(Row::from(vec![
        Cell::new("⏰ Taken At"),
        Cell::new(format_timestamp(dispute_info.taken_at)),
    ]));
    rows.push(Row::from(vec![
        Cell::new("⚡ Invoice Held At"),
        Cell::new(format_timestamp(dispute_info.invoice_held_at)),
    ]));

    table.add_rows(rows);
    table.to_string()
}

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

                    handle_new_order_display(order);
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
                handle_add_invoice_display(order);

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
                    print_success_message("Order saved successfully!");
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
                handle_pay_invoice_display(order, invoice);

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
                        print_success_message("Order saved successfully!");
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
            print_section_header("⭐ Rating Received");
            println!("🙏 Thank you for your rating!");
            println!("💡 Your feedback helps improve the trading experience");
            print_success_message("Rating processed successfully!");
            Ok(())
        }
        Action::FiatSentOk => {
            if let Some(order_id) = &message.id {
                print_section_header("💸 Fiat Payment Confirmed");
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
                print_section_header("🔢 Last Trade Index Updated");
                print_trade_index(last_trade_index as u64);
                match User::get(&ctx.pool).await {
                    Ok(mut user) => {
                        user.set_last_trade_index(last_trade_index);
                        if let Err(e) = user.save(&ctx.pool).await {
                            println!("❌ Failed to update user: {}", e);
                        } else {
                            print_success_message("Trade index synchronized successfully!");
                        }
                    }
                    Err(_) => {
                        println!("⚠️  Warning: Last trade index but received unexpected payload structure: {:#?}", message.payload);
                    }
                }
            } else {
                println!("⚠️  Warning: Last trade index but received unexpected payload structure: {:#?}", message.payload);
            }
            Ok(())
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
        Action::HoldInvoicePaymentAccepted => {
            if let Some(order_id) = &message.id {
                println!("🎉 Hold Invoice Payment Accepted");
                println!("═══════════════════════════════════════");
                println!("📋 Order ID: {}", order_id);
                println!("✅ Hold invoice payment accepted successfully!");
                Ok(())
            } else {
                println!(
                    "⚠️  Warning: Hold invoice payment accepted but received unexpected payload structure"
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
                handle_orders_list_display(orders);
            } else {
                println!(
                    "⚠️  Warning: Orders list but received unexpected payload structure: {:#?}",
                    message.payload
                );
            }
            Ok(())
        }
        Action::AdminTookDispute => {
            if let Some(Payload::Dispute(_, Some(dispute_info))) = &message.payload {
                println!("🎉 Dispute Successfully Taken!");
                println!("═══════════════════════════════════════");
                println!();

                // Display the dispute info using our dedicated function
                let dispute_table = display_solver_dispute_info(dispute_info);
                println!("{dispute_table}");
                println!();
                println!("✅ Dispute taken successfully! You are now the solver for this dispute.");
                Ok(())
            } else {
                // Fallback for debugging - show what we actually received
                println!("🎉 Dispute Successfully Taken!");
                println!("═══════════════════════════════════════");
                println!();
                println!(
                    "⚠️  Warning: Expected Dispute payload with SolverDisputeInfo but received:"
                );
                println!("📋 Payload: {:#?}", message.payload);
                println!();
                println!("✅ Dispute taken successfully! You are now the solver for this dispute.");
                Ok(())
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

        let (created_at, message, sender) = match dm.kind {
            nostr_sdk::Kind::GiftWrap => match unwrap_message(dm, pubkey).await {
                Ok(Some(u)) => (u.created_at, u.message, u.sender),
                Ok(None) => continue, // outer NIP-44 failed → not addressed to us
                Err(e) => {
                    eprintln!("Warning: could not unwrap gift wrap (event {}): {e}", dm.id);
                    continue;
                }
            },
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
                (dm.created_at, message, dm.pubkey)
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

            if created_at.as_secs() < since_time {
                continue;
            }
        }
        direct_messages.push((message, created_at.as_secs(), sender));
    }
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));
    direct_messages
}

pub async fn print_direct_messages(
    dm: &[(Message, u64, PublicKey)],
    mostro_pubkey: Option<PublicKey>,
) -> Result<()> {
    if dm.is_empty() {
        println!();
        println!("📭 No new messages");
        println!();
        return Ok(());
    }

    println!();
    print_section_header("📨 Direct Messages");

    for (i, (message, created_at, sender_pubkey)) in dm.iter().enumerate() {
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

        // From label: show 🧌 Mostro if matches provided pubkey
        let from_label = if let Some(pk) = mostro_pubkey {
            if *sender_pubkey == pk {
                format!("🧌 {}", sender_pubkey)
            } else {
                sender_pubkey.to_string()
            }
        } else {
            sender_pubkey.to_string()
        };

        // Print message header
        println!("📄 Message {}:", i + 1);
        println!("─────────────────────────────────────");
        println!("⏰ Time: {}", date);
        println!("📨 From: {}", from_label);
        println!("🎯 Action: {} {}", action_icon, action_str);

        // Print details with proper formatting
        if let Some(payload) = &inner.payload {
            let details = format_payload_details(payload, &inner.action);
            println!("📝 Details:");
            for line in details.lines() {
                println!("   {}", line);
            }
        } else {
            println!("📝 Details: -");
        }

        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {}
