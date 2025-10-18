use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;

/// Apply color coding to status cells based on status type
pub fn apply_status_color(cell: Cell, status: &str) -> Cell {
    let status_lower = status.to_lowercase();

    if status_lower.contains("init")
        || status_lower.contains("pending")
        || status_lower.contains("waiting")
    {
        cell.fg(Color::Yellow)
    } else if status_lower.contains("active")
        || status_lower.contains("released")
        || status_lower.contains("settled")
        || status_lower.contains("taken")
        || status_lower.contains("success")
    {
        cell.fg(Color::Green)
    } else if status_lower.contains("fiat") {
        cell.fg(Color::Cyan)
    } else if status_lower.contains("dispute")
        || status_lower.contains("cancel")
        || status_lower.contains("canceled")
    {
        cell.fg(Color::Red)
    } else {
        cell
    }
}

/// Apply color coding to order kind cells
pub fn apply_kind_color(cell: Cell, kind: &mostro_core::order::Kind) -> Cell {
    match kind {
        mostro_core::order::Kind::Buy => cell.fg(Color::Green),
        mostro_core::order::Kind::Sell => cell.fg(Color::Red),
    }
}

/// Create a red error cell for "no data found" messages
pub fn create_error_cell(message: &str) -> Cell {
    Cell::new(message)
        .fg(Color::Red)
        .set_alignment(CellAlignment::Center)
}

/// Create a standard table with UTF8_FULL preset and dynamic arrangement
pub fn create_standard_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}

/// Create a standard field/value table header
pub fn create_field_value_header() -> Vec<Cell> {
    vec![
        Cell::new("Field")
            .add_attribute(Attribute::Bold)
            .set_alignment(CellAlignment::Center),
        Cell::new("Value")
            .add_attribute(Attribute::Bold)
            .set_alignment(CellAlignment::Center),
    ]
}

/// Create a centered cell with optional bold formatting
pub fn create_centered_cell(content: &str, bold: bool) -> Cell {
    let mut cell = Cell::new(content).set_alignment(CellAlignment::Center);
    if bold {
        cell = cell.add_attribute(Attribute::Bold);
    }
    cell
}

/// Create a field/value row for tables
pub fn create_field_value_row(field: &str, value: &str) -> Row {
    Row::from(vec![
        Cell::new(field).set_alignment(CellAlignment::Center),
        Cell::new(value).set_alignment(CellAlignment::Center),
    ])
}

/// Format timestamp to human-readable string
pub fn format_timestamp(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Invalid timestamp".to_string())
}

/// Create a field/value row with emoji field
pub fn create_emoji_field_row(emoji: &str, field: &str, value: &str) -> Row {
    Row::from(vec![
        Cell::new(format!("{}{}", emoji, field)).set_alignment(CellAlignment::Center),
        Cell::new(value).set_alignment(CellAlignment::Center),
    ])
}

/// Print a standard section header with title and separator
pub fn print_section_header(title: &str) {
    println!("{}", title);
    println!("═══════════════════════════════════════");
}

/// Print a success message with consistent formatting
pub fn print_success_message(message: &str) {
    println!("✅ {}", message);
}

/// Print an info message with consistent formatting
pub fn print_info_message(message: &str) {
    println!("💡 {}", message);
}

/// Print a no-data message with consistent formatting
pub fn print_no_data_message(message: &str) {
    println!("📭 {}", message);
}

/// Print a key-value pair with consistent formatting
pub fn print_key_value(emoji: &str, key: &str, value: &str) {
    println!("{} {}: {}", emoji, key, value);
}

/// Print a simple info line with consistent formatting
pub fn print_info_line(emoji: &str, message: &str) {
    println!("{} {}", emoji, message);
}

/// Print order information with consistent formatting
pub fn print_order_info(
    order_id: &str,
    amount: i64,
    fiat_code: &str,
    premium: i64,
    payment_method: &str,
) {
    println!("📋 Order ID: {}", order_id);
    println!("💰 Amount: {} sats", amount);
    println!("💱 Fiat Code: {}", fiat_code);
    println!("📊 Premium: {}%", premium);
    println!("💳 Payment Method: {}", payment_method);
}

/// Print order status information
pub fn print_order_status(status: &str) {
    println!("📊 Status: {}", status);
}

/// Print amount information
pub fn print_amount_info(amount: i64) {
    println!("💰 Amount: {} sats", amount);
}

/// Print required amount information
pub fn print_required_amount(amount: i64) {
    println!("💰 Required Amount: {} sats", amount);
}

/// Print fiat code information
pub fn print_fiat_code(fiat_code: &str) {
    println!("💱 Fiat Code: {}", fiat_code);
}

/// Print premium information
pub fn print_premium(premium: i64) {
    println!("📊 Premium: {}%", premium);
}

/// Print payment method information
pub fn print_payment_method(payment_method: &str) {
    println!("💳 Payment Method: {}", payment_method);
}

/// Print trade index information
pub fn print_trade_index(trade_index: u64) {
    println!("🔢 Last Trade Index: {}", trade_index);
}

/// Print order count information
pub fn print_order_count(count: usize) {
    println!("📊 Found {} order(s):", count);
}
