use mostro_client::parser::orders::{parse_orders_events, print_orders_table};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::BTreeSet;

fn build_order_event(
    kind: mostro_core::order::Kind,
    status: Status,
    fiat: &str,
    amount: i64,
    fiat_amount: i64,
) -> Event {
    let keys = Keys::generate();
    let id = uuid::Uuid::new_v4();

    let mut tags = Tags::new();
    tags.push(Tag::custom("d", vec![id.to_string()]));
    tags.push(Tag::custom("k", vec![kind.to_string()]));
    tags.push(Tag::custom("f", vec![fiat.to_string()]));
    tags.push(Tag::custom("s", vec![status.to_string()]));
    tags.push(Tag::custom("amt", vec![amount.to_string()]));
    tags.push(Tag::custom("fa", vec![fiat_amount.to_string()]));

    EventBuilder::new(nostr_sdk::prelude::Kind::TextNote, "")
        .tags(tags)
        .finalize(&keys)
        .unwrap()
}

#[test]
fn parse_orders_empty() {
    let events = BTreeSet::new();
    let out = parse_orders_events(events, None, None, None);
    assert!(out.is_empty());
}

#[test]
fn parse_orders_basic_and_print() {
    let e = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Pending,
        "USD",
        100,
        1000,
    );
    let mut events = BTreeSet::new();
    events.insert(e);
    let out = parse_orders_events(
        events,
        Some("USD".into()),
        Some(Status::Pending),
        Some(mostro_core::order::Kind::Sell),
    );
    assert_eq!(out.len(), 1);

    let printable = out
        .into_iter()
        .map(mostro_client::util::Event::SmallOrder)
        .collect::<Vec<_>>();
    let table = print_orders_table(printable).expect("table should render");
    assert!(table.contains("USD"));
}

#[test]
fn parse_orders_with_kind_filter() {
    let e1 = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "USD",
        100000,
        1000,
    );
    let e2 = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Active,
        "USD",
        100000,
        1000,
    );
    let mut events = BTreeSet::new();
    events.insert(e1);
    events.insert(e2);

    let out = parse_orders_events(
        events,
        Some("USD".into()),
        Some(Status::Active),
        Some(mostro_core::order::Kind::Buy),
    );

    // Should only return Buy orders
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_orders_with_status_filter() {
    let e1 = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Active,
        "EUR",
        50000,
        500,
    );
    let e2 = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Pending,
        "EUR",
        50000,
        500,
    );
    let mut events = BTreeSet::new();
    events.insert(e1);
    events.insert(e2);

    let out = parse_orders_events(events, Some("EUR".into()), Some(Status::Active), None);

    // Should only return Active orders
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_orders_with_currency_filter() {
    let e1 = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "USD",
        100000,
        1000,
    );
    let e2 = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "EUR",
        100000,
        1000,
    );
    let mut events = BTreeSet::new();
    events.insert(e1);
    events.insert(e2);

    let out = parse_orders_events(events, Some("USD".into()), Some(Status::Active), None);

    // Should only return USD orders
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_orders_no_filters() {
    let e1 = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "USD",
        100000,
        1000,
    );
    let e2 = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Pending,
        "EUR",
        50000,
        500,
    );
    let mut events = BTreeSet::new();
    events.insert(e1);
    events.insert(e2);

    let out = parse_orders_events(events, None, None, None);

    // Should return all orders
    assert_eq!(out.len(), 2);
}

#[test]
fn print_orders_empty_list() {
    let orders: Vec<mostro_client::util::Event> = Vec::new();
    let table = print_orders_table(orders);

    assert!(table.is_ok());
    let table_str = table.unwrap();
    assert!(table_str.contains("No offers found"));
}

#[test]
fn print_orders_multiple_orders() {
    let orders = vec![
        build_order_event(
            mostro_core::order::Kind::Buy,
            Status::Active,
            "USD",
            100000,
            1000,
        ),
        build_order_event(
            mostro_core::order::Kind::Sell,
            Status::Pending,
            "EUR",
            50000,
            500,
        ),
    ];

    let mut events = BTreeSet::new();
    for order in orders {
        events.insert(order);
    }

    let parsed = parse_orders_events(events, None, None, None);
    let printable = parsed
        .into_iter()
        .map(mostro_client::util::Event::SmallOrder)
        .collect::<Vec<_>>();

    let table = print_orders_table(printable);
    assert!(table.is_ok());

    let table_str = table.unwrap();
    assert!(table_str.contains("USD") || table_str.contains("EUR"));
}

#[test]
fn parse_orders_different_amounts() {
    let amounts = vec![10000i64, 50000i64, 100000i64, 1000000i64];
    let mut events = BTreeSet::new();

    for amount in &amounts {
        let e = build_order_event(
            mostro_core::order::Kind::Buy,
            Status::Active,
            "USD",
            *amount,
            *amount / 100_i64,
        );
        events.insert(e);
    }

    let out = parse_orders_events(events, Some("USD".into()), None, None);
    assert_eq!(out.len(), amounts.len());
}

#[test]
fn parse_orders_different_currencies() {
    let currencies = vec!["USD", "EUR", "GBP", "JPY", "CAD"];
    let mut events = BTreeSet::new();

    for currency in &currencies {
        let e = build_order_event(
            mostro_core::order::Kind::Sell,
            Status::Active,
            currency,
            100000,
            1000,
        );
        events.insert(e);
    }

    let out = parse_orders_events(events, None, None, None);
    assert_eq!(out.len(), currencies.len());
}

#[test]
fn parse_orders_market_price() {
    // Market price orders have amount = 0
    let e = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "USD",
        0,
        1000,
    );
    let mut events = BTreeSet::new();
    events.insert(e);

    let out = parse_orders_events(events, Some("USD".into()), None, None);
    assert_eq!(out.len(), 1);
}
