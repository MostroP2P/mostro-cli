//! Order table shared by the legacy listing and the typed `order.list` view,
//! and the escaping of received text for the terminal.

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
        centered(escape_controls(&order.fiat_currency)),
        centered(fiat_amount(
            order.fiat_amount,
            order.fiat_amount_min,
            order.fiat_amount_max,
        )),
        centered(escape_controls(&order.payment_method)),
        centered(order.premium_percent),
        centered(created),
    ])
}

/// Received text with every control character shown as a Rust escape, so
/// public data cannot move the cursor, recolor or break table lines.
pub fn escape_controls(text: &str) -> String {
    escape_where(text, char::is_control)
}

/// Text a person wrote (chat), escaped like [`escape_controls`] and also for
/// every invisible character that can reorder, hide or split what a terminal
/// shows: the format characters (bidirectional overrides and isolates,
/// zero-width spaces, the byte order mark, tags) and the line and paragraph
/// separators. The joiners U+200C and U+200D stay: emoji and Persian need them.
pub fn escape_untrusted(text: &str) -> String {
    escape_where(text, |c| {
        c.is_control()
            || INVISIBLE
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&c))
    })
}

/// The Unicode 17.0 format characters (general category Cf, 21 ranges, as in
/// `unicode-properties` 0.1.4) except U+200C and U+200D, then the line
/// separator (Zl) and the paragraph separator (Zp). A format character added
/// by a later Unicode version is not escaped until this table is updated.
const INVISIBLE: [(char, char); 24] = [
    ('\u{00AD}', '\u{00AD}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{06DD}', '\u{06DD}'),
    ('\u{070F}', '\u{070F}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08E2}', '\u{08E2}'),
    ('\u{180E}', '\u{180E}'),
    ('\u{200B}', '\u{200B}'),
    ('\u{200E}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFF9}', '\u{FFFB}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0001}', '\u{E0001}'),
    ('\u{E0020}', '\u{E007F}'),
    ('\u{2028}', '\u{2028}'),
    ('\u{2029}', '\u{2029}'),
];

/// `text` with every character `escaped` selects shown as a Rust escape.
fn escape_where(text: &str, escaped: impl Fn(char) -> bool) -> String {
    text.chars()
        .map(|c| {
            if escaped(c) {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_text_escapes_format_characters_and_line_separators() {
        // One character from each of the 21 Unicode 17.0 Cf ranges, two from U+200B-U+200F.
        let format = "\u{00AD}\u{0603}\u{061C}\u{06DD}\u{070F}\u{0890}\u{08E2}\u{180E}\u{200B}\u{200E}\
            \u{202E}\u{2064}\u{2066}\u{FEFF}\u{FFFA}\u{110BD}\u{110CD}\u{13430}\u{1BCA0}\u{1D173}\u{E0001}\u{E0041}";
        assert_eq!(format.chars().count(), 22);
        for c in format.chars().chain(['\u{2028}', '\u{2029}']) {
            let text = format!("a{c}b");
            let escaped = escape_untrusted(&text);
            assert_eq!(escaped, format!("a{}b", c.escape_unicode()), "{c:?}");
            assert_eq!(escape_controls(&text), text, "escape_controls keeps {c:?}");
        }
        let kept = "\u{00AC}\u{0892}\u{1D17B} 💬 🤷\u{200D}♀\u{FE0F} می\u{200C}خواهم ñandú café";
        assert_eq!(escape_untrusted(kept), kept, "emoji, joiners, accents");
        assert_eq!(escape_untrusted("a\u{1b}[31m\r"), "a\\u{1b}[31m\\r");
        assert_eq!(escape_controls("a\u{1b}[31m\r"), "a\\u{1b}[31m\\r");
    }
}
