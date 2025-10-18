use std::collections::HashMap;

use crate::parser::common::{apply_kind_color, apply_status_color, create_error_cell};
use crate::util::Event;
use anyhow::Result;
use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use log::{error, info};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::nip33::order_from_tags;

pub fn parse_orders_events(
    events: Events,
    currency: Option<String>,
    status: Option<Status>,
    kind: Option<mostro_core::order::Kind>,
) -> Vec<SmallOrder> {
    // HashMap to store the latest order by id
    let mut latest_by_id: HashMap<Uuid, SmallOrder> = HashMap::new();

    for event in events.iter() {
        // Get order from tags
        let mut order = match order_from_tags(event.tags.clone()) {
            Ok(o) => o,
            Err(e) => {
                error!("{e:?}");
                continue;
            }
        };
        // Get order id
        let order_id = match order.id {
            Some(id) => id,
            None => {
                info!("Order ID is none");
                continue;
            }
        };
        // Check if order kind is none
        if order.kind.is_none() {
            info!("Order kind is none");
            continue;
        }
        // Set created at
        order.created_at = Some(event.created_at.as_u64() as i64);
        // Update latest order by id
        latest_by_id
            .entry(order_id)
            .and_modify(|existing| {
                let new_ts = order.created_at.unwrap_or(0);
                let old_ts = existing.created_at.unwrap_or(0);
                if new_ts > old_ts {
                    *existing = order.clone();
                }
            })
            .or_insert(order);
    }

    let mut requested: Vec<SmallOrder> = latest_by_id
        .into_values()
        .filter(|o| status.map(|s| o.status == Some(s)).unwrap_or(true))
        .filter(|o| currency.as_ref().map(|c| o.fiat_code == *c).unwrap_or(true))
        .filter(|o| {
            kind.as_ref()
                .map(|k| o.kind.as_ref() == Some(k))
                .unwrap_or(true)
        })
        .collect();

    requested.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    requested
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
            Cell::new("📈 Buy/Sell")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💰 Sats Amount")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💱 Fiat Code")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💵 Fiat Amount")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💳 Payment Method")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("📊 Premium %")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);

    //Table rows
    let r = Row::from(vec![
        if let Some(k) = single_order.kind {
            apply_kind_color(
                Cell::new(k.to_string()).set_alignment(CellAlignment::Center),
                &k,
            )
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

    let mut result = table.to_string();
    result.push('\n');
    result.push_str("═══════════════════════════════════════\n");
    result.push_str("📋 Order Preview - Please review carefully\n");
    result.push_str("💡 This order will be submitted to Mostro\n");
    result.push_str("✅ All details look correct? (Y/n)\n");
    result.push_str("═══════════════════════════════════════\n");

    Ok(result)
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
            .set_header(vec![Cell::new("📭 No Offers")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center)]);

        // Single row for error
        let mut r = Row::new();

        r.add_cell(create_error_cell(
            "No offers found with requested parameters…",
        ));

        //Push single error row
        rows.push(r);
    } else {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![
                Cell::new("📈 Buy/Sell")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("🆔 Order Id")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("📊 Status")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("💰 Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("💱 Fiat Code")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("💵 Fiat Amount")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("💳 Payment Method")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("📅 Created")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
            ]);

        //Iterate to create table of orders
        for single_order in orders_table.into_iter() {
            let date = DateTime::from_timestamp(single_order.created_at.unwrap_or(0), 0);

            let r = Row::from(vec![
                if let Some(k) = single_order.kind {
                    apply_kind_color(
                        Cell::new(k.to_string()).set_alignment(CellAlignment::Center),
                        &k,
                    )
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
                {
                    let status = single_order
                        .status
                        .unwrap_or(mostro_core::order::Status::Active)
                        .to_string();
                    apply_status_color(
                        Cell::new(&status).set_alignment(CellAlignment::Center),
                        &status,
                    )
                },
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
                )
                .set_alignment(CellAlignment::Center),
            ]);
            rows.push(r);
        }
    }

    table.add_rows(rows);

    Ok(table.to_string())
}

#[cfg(test)]
mod tests {}
