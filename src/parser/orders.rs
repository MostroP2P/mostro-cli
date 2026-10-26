use std::collections::BTreeSet;

use crate::msg::reduce_orders;
use crate::parse_human::table::{fiat_amount, orders_table as orders_table_text, OrderRow};
use crate::parser::common::apply_kind_color;
use crate::util::Event;
use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use mostro_core::prelude::*;

pub fn parse_orders_events(
    events: BTreeSet<nostr_sdk::prelude::Event>,
    currency: Option<String>,
    status: Option<Status>,
    kind: Option<mostro_core::order::Kind>,
) -> Vec<SmallOrder> {
    reduce_orders(&events, currency.as_deref(), status, kind)
}

pub fn print_order_preview(ord: Payload) -> Result<String, String> {
    let single_order = match ord {
        Payload::Order(o) => o,
        _ => return Err("Error".to_string()),
    };

    let mut table = Table::new();

    table
        .load_style(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(160)
        .set_header(vec![
            Cell::new("📈 Kind")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("₿ Amount")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💱 Fiat")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("💵 Fiat Amt")
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
            Cell::new("market").set_alignment(CellAlignment::Center)
        } else {
            Cell::new(single_order.amount).set_alignment(CellAlignment::Center)
        },
        Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
        Cell::new(fiat_amount(
            single_order.fiat_amount.to_string(),
            single_order.min_amount.map(|amount| amount.to_string()),
            single_order.max_amount.map(|amount| amount.to_string()),
        ))
        .set_alignment(CellAlignment::Center),
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
    let rows = orders_table
        .into_iter()
        .filter_map(|event| match event {
            Event::SmallOrder(order) => Some(legacy_row(order)),
            _ => None,
        })
        .collect();
    Ok(orders_table_text(rows))
}

/// Legacy rows show an absent status as active and an absent time as 1970.
fn legacy_row(order: SmallOrder) -> OrderRow {
    OrderRow {
        side: order.kind,
        id: order
            .id
            .map_or_else(|| "N/A".to_string(), |id| id.to_string()),
        status: order.status.unwrap_or(Status::Active).to_string(),
        sats_amount: order.amount.to_string(),
        fiat_currency: order.fiat_code,
        fiat_amount: order.fiat_amount.to_string(),
        fiat_amount_min: order.min_amount.map(|amount| amount.to_string()),
        fiat_amount_max: order.max_amount.map(|amount| amount.to_string()),
        payment_method: order.payment_method,
        premium_percent: order.premium.to_string(),
        created_at_unix: Some(order.created_at.unwrap_or(0)),
    }
}

#[cfg(test)]
mod tests {}
