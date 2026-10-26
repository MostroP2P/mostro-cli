//! Order table shared by the legacy listing and the typed `order.list` view.

use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{
    Attribute, Cell, CellAlignment, ColumnConstraint, ContentArrangement, Row, Table, Width,
};
use mostro_core::order::Kind;

use crate::parser::common::{apply_kind_color, apply_status_color, create_error_cell};

const TABLE_WIDTH: u16 = 160;
const ID_COLUMN_WIDTH: u16 = 38;
const HEADINGS: [&str; 9] = [
    "📈 Kind",
    "🆔 Order Id",
    "📊 Status",
    "₿ Amount",
    "💱 Fiat",
    "💵 Fiat Amt",
    "💳 Payment Method",
    "📊 Premium %",
    "📅 Created",
];

/// One order as displayed; amounts are canonical decimal strings.
pub struct OrderRow {
    pub side: Option<Kind>,
    pub id: String,
    pub status: String,
    pub sats_amount: String,
    pub fiat_currency: String,
    pub fiat_amount: String,
    pub fiat_amount_min: Option<String>,
    pub fiat_amount_max: Option<String>,
    pub payment_method: String,
    pub premium_percent: String,
    /// Decoded event time; `None` displays as an invalid date.
    pub created_at_unix: Option<i64>,
}

/// The order table, or a single "no offers" row when `rows` is empty.
pub fn orders_table(rows: Vec<OrderRow>) -> String {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(TABLE_WIDTH);

    if rows.is_empty() {
        table.set_header(vec![heading("📭 No Offers")]);
        table.add_row(Row::from(vec![create_error_cell(
            "No offers found with requested parameters…",
        )]));
        return table.to_string();
    }

    table.set_header(HEADINGS.map(heading).to_vec());
    table.add_rows(rows.into_iter().map(order_row));
    // Keep UUIDs from being truncated.
    if let Some(column) = table.column_mut(1) {
        column.set_constraint(ColumnConstraint::Absolute(Width::Fixed(ID_COLUMN_WIDTH)));
    }
    table.to_string()
}

fn heading(text: &str) -> Cell {
    Cell::new(text)
        .add_attribute(Attribute::Bold)
        .set_alignment(CellAlignment::Center)
}

fn centered(text: impl ToString) -> Cell {
    Cell::new(text).set_alignment(CellAlignment::Center)
}

fn order_row(order: OrderRow) -> Row {
    let side = match order.side {
        Some(side) => apply_kind_color(centered(side), &side),
        None => centered("BUY/SELL"),
    };
    let sats = if order.sats_amount == "0" {
        "market".to_string()
    } else {
        order.sats_amount
    };
    let created = order
        .created_at_unix
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .map(|at| at.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "Invalid date".to_string());
    Row::from(vec![
        side,
        centered(order.id),
        apply_status_color(centered(&order.status), &order.status),
        centered(sats),
        centered(order.fiat_currency),
        centered(fiat_amount(
            order.fiat_amount,
            order.fiat_amount_min,
            order.fiat_amount_max,
        )),
        centered(order.payment_method),
        centered(order.premium_percent),
        centered(created),
    ])
}

/// The fixed amount, or the range with `?` for a missing endpoint.
pub fn fiat_amount(fixed: String, min: Option<String>, max: Option<String>) -> String {
    match (min, max) {
        (None, None) => fixed,
        (min, max) => format!(
            "{}-{}",
            min.as_deref().unwrap_or("?"),
            max.as_deref().unwrap_or("?")
        ),
    }
}
