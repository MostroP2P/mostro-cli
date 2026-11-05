//! Human view of `order.list` outcomes, built only from the typed data.

use std::fmt::Write;

use crate::outcome::order::{ListData, PublicOrder};

use super::table::{orders_table, OrderRow};

/// The filter summary, node key and order table of a listing.
pub fn render_list(data: &ListData) -> String {
    let filters = &data.filters;
    let mut text = String::from("📋 List Orders\n═══════════════════════════════════════\n");
    let _ = writeln!(text, "📊 Status Filter: {}", filters.status());
    if let Some(side) = filters.side() {
        let _ = writeln!(text, "📈 Order Type: {side} orders");
    }
    if let Some(currency) = filters.currency() {
        let _ = writeln!(text, "💱 Currency Filter: {currency}");
    }
    let _ = writeln!(text, "🎯 Mostro PubKey: {}\n", data.node_pubkey);
    text.push_str(&orders_table(data.orders.iter().map(row).collect()));
    text.push('\n');
    text
}

fn row(order: &PublicOrder) -> OrderRow {
    OrderRow {
        side: Some(order.side),
        id: order.id.clone(),
        status: order.status.to_string(),
        sats_amount: order.sats_amount.clone(),
        fiat_currency: order.fiat_currency.clone(),
        fiat_amount: order.fiat_amount.clone(),
        fiat_amount_min: order.fiat_amount_min.clone(),
        fiat_amount_max: order.fiat_amount_max.clone(),
        payment_method: order.payment_method.clone(),
        premium_percent: order.premium_percent.clone(),
        created_at_unix: order.created_at_unix.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::order::ListOrdersInput;
    use crate::parser::orders::print_orders_table;
    use crate::util::Event;
    use mostro_core::order::{Kind, SmallOrder, Status};
    use nostr_sdk::prelude::Keys;
    use std::slice::from_ref;
    use uuid::Uuid;

    fn order(created_at: i64, min: Option<i64>, max: Option<i64>) -> SmallOrder {
        SmallOrder {
            id: Some(Uuid::from_u128(7)),
            kind: Some(Kind::Buy),
            status: Some(Status::Pending),
            fiat_code: "EUR".to_string(),
            fiat_amount: 50,
            min_amount: min,
            max_amount: max,
            payment_method: "sepa".to_string(),
            premium: -2,
            created_at: Some(created_at),
            ..SmallOrder::default()
        }
    }

    fn listing(filters: ListOrdersInput, orders: &[SmallOrder]) -> (String, ListData) {
        let node = Keys::generate().public_key();
        let (data, _) = ListData::from_selected(&node, filters, orders).unwrap();
        (render_list(&data), data)
    }

    #[test]
    fn typed_and_legacy_views_share_one_table() {
        let selected = order(1_700_000_000, Some(10), None);
        let filters = ListOrdersInput::parse(Some("buy"), Some("eur"), None).unwrap();
        let (text, data) = listing(filters, from_ref(&selected));
        let legacy = print_orders_table(vec![Event::SmallOrder(selected)]).unwrap();

        assert!(text.ends_with(&format!("{legacy}\n")), "{text}");
        for expected in [
            "Status Filter: pending",
            "Order Type: buy orders",
            "Currency Filter: EUR",
            &data.node_pubkey,
            "00000000-0000-0000-0000-000000000007",
            "market",
            "10-?",
            "2023-11-14 22:13",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn empty_and_unrepresentable_rows_keep_the_existing_wording() {
        let pending = ListOrdersInput::parse(None, None, None).unwrap();
        let (empty, _) = listing(pending.clone(), &[]);
        assert!(empty.contains("No offers found with requested parameters"));
        assert!(!empty.contains("Currency Filter"));

        let (text, _) = listing(pending, &[order(i64::MAX, None, None)]);
        assert!(text.contains("Invalid date"), "{text}");
        assert!(text.contains("50"), "{text}");
    }
}
