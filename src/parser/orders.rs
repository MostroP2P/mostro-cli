use crate::util::Event;
use anyhow::Result;
use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use log::{error, info};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::nip33::order_from_tags;

pub fn parse_orders_events(
    events: Events,
    currency: Option<String>,
    status: Option<Status>,
    kind: Option<mostro_core::order::Kind>,
) -> Vec<SmallOrder> {
    // Extracted Orders List
    let mut complete_events_list = Vec::<SmallOrder>::new();
    let mut requested_orders_list = Vec::<SmallOrder>::new();

    // Scan events to extract all orders
    for event in events.iter() {
        let order = order_from_tags(event.tags.clone());

        if order.is_err() {
            error!("{order:?}");
            continue;
        }
        if let Ok(mut order) = order {
            if let Some(order_id) = order.id {
                info!("Found Order id : {:?}", order_id);
            } else {
                info!("Order ID is none");
                continue;
            }

            // Check if order kind is none
            if order.kind.is_none() {
                info!("Order kind is none");
                continue;
            }

            // Check if order status is none
            if let Some(filter_status) = status {
                if order.status != Some(filter_status) {
                    continue;
                }
            }
            // Check if order fiat code is none
            if let Some(ref curr) = currency {
                if order.fiat_code != *curr {
                    continue;
                }
            }

            // Get created at field from Nostr event
            if let Some(ref k) = kind {
                if order.kind.as_ref() != Some(k) {
                    continue;
                }
            }

            // Get created at field from Nostr event
            order.created_at = Some(event.created_at.as_u64() as i64);
            complete_events_list.push(order.clone());

            // Add just requested orders requested by filtering
            requested_orders_list.push(order);
        }
        // Order all element ( orders ) received to filter - discard disaligned messages
        // if an order has an older message with the state we received is discarded for the latest one
        requested_orders_list.retain(|keep| {
            !complete_events_list
                .iter()
                .any(|x| x.id == keep.id && x.created_at > keep.created_at)
        });
        // Sort by id to remove duplicates
        requested_orders_list.sort_by(|a, b| b.id.cmp(&a.id));
        requested_orders_list.dedup_by(|a, b| a.id == b.id);
    }
    // Finally sort list by creation time
    requested_orders_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    requested_orders_list
}

pub fn print_order_preview(ord: Payload) -> Result<String, String> {
    let single_order = match ord {
        Payload::Order(o) => o,
        _ => return Err("Error".to_string()),
    };

    let mut table = Table::new();

    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(160)
        .set_header(vec![
            Cell::new("Buy/Sell")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Sats Amount")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Fiat Code")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Fiat Amount")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Payment method")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Premium %")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);

    //Table rows
    let r = Row::from(vec![
        if let Some(k) = single_order.kind {
            match k {
                mostro_core::order::Kind::Buy => Cell::new(k.to_string())
                    .fg(Color::Green)
                    .set_alignment(CellAlignment::Center),
                mostro_core::order::Kind::Sell => Cell::new(k.to_string())
                    .fg(Color::Red)
                    .set_alignment(CellAlignment::Center),
            }
        } else {
            Cell::new("BUY/SELL").set_alignment(CellAlignment::Center)
        },
        if single_order.amount == 0 {
            Cell::new("market price").set_alignment(CellAlignment::Center)
        } else {
            Cell::new(single_order.amount).set_alignment(CellAlignment::Center)
        },
        Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
        // No range order print row
        if single_order.min_amount.is_none() && single_order.max_amount.is_none() {
            Cell::new(single_order.fiat_amount.to_string()).set_alignment(CellAlignment::Center)
        } else {
            let range_str = match (single_order.min_amount, single_order.max_amount) {
                (Some(min), Some(max)) => format!("{}-{}", min, max),
                (Some(min), None) => format!("{}-?", min),
                (None, Some(max)) => format!("?-{}", max),
                (None, None) => "?".to_string(),
            };
            Cell::new(range_str).set_alignment(CellAlignment::Center)
        },
        Cell::new(single_order.payment_method.to_string()).set_alignment(CellAlignment::Center),
        Cell::new(single_order.premium.to_string()).set_alignment(CellAlignment::Center),
    ]);

    table.add_row(r);

    Ok(table.to_string())
}

pub fn print_orders_table(orders_table: Vec<Event>) -> Result<String> {
    let mut table = Table::new();
    // Convert Event to SmallOrder
    let orders_table: Vec<SmallOrder> = orders_table
        .into_iter()
        .filter_map(|event| {
            if let Event::SmallOrder(order) = event {
                Some(order)
            } else {
                None
            }
        })
        .collect();

    //Table rows
    let mut rows: Vec<Row> = Vec::new();

    if orders_table.is_empty() {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![Cell::new("Sorry...")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center)]);

        // Single row for error
        let mut r = Row::new();

        r.add_cell(
            Cell::new("No offers found with requested parameters...")
                .fg(Color::Red)
                .set_alignment(CellAlignment::Center),
        );

        //Push single error row
        rows.push(r);
    } else {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![
                Cell::new("Buy/Sell")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Order Id")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Status")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Fiat Code")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Fiat Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Payment method")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("Created")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
            ]);

        //Iterate to create table of orders
        for single_order in orders_table.into_iter() {
            let date = DateTime::from_timestamp(single_order.created_at.unwrap_or(0), 0);

            let r = Row::from(vec![
                if let Some(k) = single_order.kind {
                    match k {
                        mostro_core::order::Kind::Buy => Cell::new(k.to_string())
                            .fg(Color::Green)
                            .set_alignment(CellAlignment::Center),
                        mostro_core::order::Kind::Sell => Cell::new(k.to_string())
                            .fg(Color::Red)
                            .set_alignment(CellAlignment::Center),
                    }
                } else {
                    Cell::new("BUY/SELL").set_alignment(CellAlignment::Center)
                },
                Cell::new(
                    single_order
                        .id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                )
                .set_alignment(CellAlignment::Center),
                Cell::new(
                    single_order
                        .status
                        .unwrap_or(mostro_core::order::Status::Active)
                        .to_string(),
                )
                .set_alignment(CellAlignment::Center),
                if single_order.amount == 0 {
                    Cell::new("market price").set_alignment(CellAlignment::Center)
                } else {
                    Cell::new(single_order.amount.to_string()).set_alignment(CellAlignment::Center)
                },
                Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
                // No range order print row
                if single_order.min_amount.is_none() && single_order.max_amount.is_none() {
                    Cell::new(single_order.fiat_amount.to_string())
                        .set_alignment(CellAlignment::Center)
                } else {
                    let range_str = match (single_order.min_amount, single_order.max_amount) {
                        (Some(min), Some(max)) => format!("{}-{}", min, max),
                        (Some(min), None) => format!("{}-?", min),
                        (None, Some(max)) => format!("?-{}", max),
                        (None, None) => "?".to_string(),
                    };
                    Cell::new(range_str).set_alignment(CellAlignment::Center)
                },
                Cell::new(single_order.payment_method.to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(
                    date.map(|d| d.to_string())
                        .unwrap_or_else(|| "Invalid date".to_string()),
                ),
            ]);
            rows.push(r);
        }
    }

    table.add_rows(rows);

    Ok(table.to_string())
}
