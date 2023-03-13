use crate::types::{Content, Order};
use anyhow::Result;
use chrono::NaiveDateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;

pub fn print_order_preview(ord: Content) -> Result<String, String> {
    let single_order = match ord {
        Content::Order(o) => o,
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
        match single_order.kind {
            crate::types::Kind::Buy => Cell::new(single_order.kind.to_string())
                .fg(Color::Green)
                .set_alignment(CellAlignment::Center),
            crate::types::Kind::Sell => Cell::new(single_order.kind.to_string())
                .fg(Color::Red)
                .set_alignment(CellAlignment::Center),
        },
        if single_order.amount.is_none() {
            Cell::new("market price").set_alignment(CellAlignment::Center)
        } else {
            Cell::new(single_order.amount.unwrap()).set_alignment(CellAlignment::Center)
        },
        Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
        Cell::new(single_order.fiat_amount.to_string()).set_alignment(CellAlignment::Center),
        Cell::new(single_order.payment_method.to_string()).set_alignment(CellAlignment::Center),
        Cell::new(single_order.prime.to_string()).set_alignment(CellAlignment::Center),
    ]);

    table.add_row(r);

    Ok(table.to_string())
}

pub fn print_orders_table(orders_table: Vec<Order>) -> Result<String> {
    let mut table = Table::new();

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
            let date =
                NaiveDateTime::from_timestamp_opt(single_order.created_at.unwrap_or(0) as i64, 0);

            let r = Row::from(vec![
                // Cell::new(single_order.kind.to_string()),
                match single_order.kind {
                    crate::types::Kind::Buy => Cell::new(single_order.kind.to_string())
                        .fg(Color::Green)
                        .set_alignment(CellAlignment::Center),
                    crate::types::Kind::Sell => Cell::new(single_order.kind.to_string())
                        .fg(Color::Red)
                        .set_alignment(CellAlignment::Center),
                },
                Cell::new(single_order.id.unwrap()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.status.to_string()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.amount.unwrap().to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(single_order.fiat_code.to_string()).set_alignment(CellAlignment::Center),
                Cell::new(single_order.fiat_amount.to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(single_order.payment_method.to_string())
                    .set_alignment(CellAlignment::Center),
                Cell::new(date.unwrap()),
            ]);
            rows.push(r);
        }
    }

    table.add_rows(rows);

    Ok(table.to_string())
}
