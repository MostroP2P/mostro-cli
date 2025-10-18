use mostro_client::parser::orders::{parse_orders_events, print_orders_table};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

fn build_order_event(
    kind: mostro_core::order::Kind,
    status: Status,
    fiat: &str,
    amount: i64,
    fiat_amount: i64,
) -> nostr_sdk::Event {
    let keys = Keys::generate();
    let id = uuid::Uuid::new_v4();

    let mut tags = Tags::new();
    tags.push(Tag::custom(
        TagKind::Custom("d".into()),
        vec![id.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("k".into()),
        vec![kind.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("f".into()),
        vec![fiat.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("s".into()),
        vec![status.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("amt".into()),
        vec![amount.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("fa".into()),
        vec![fiat_amount.to_string()],
    ));

    EventBuilder::new(nostr_sdk::Kind::TextNote, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .unwrap()
}

#[test]
fn parse_orders_empty() {
    let filter = Filter::new();
    let events = Events::new(&filter);
    let out = parse_orders_events(events, None, None, None);
    assert!(out.is_empty());
}

#[test]
fn parse_orders_basic_and_print() {
    let filter = Filter::new();
    let e = build_order_event(
        mostro_core::order::Kind::Sell,
        Status::Pending,
        "USD",
        100,
        1000,
    );
    let mut events = Events::new(&filter);
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
    let filter = Filter::new();
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
    let mut events = Events::new(&filter);
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
    let filter = Filter::new();
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
    let mut events = Events::new(&filter);
    events.insert(e1);
    events.insert(e2);
    
    let out = parse_orders_events(
        events,
        Some("EUR".into()),
        Some(Status::Active),
        None,
    );
    
    // Should only return Active orders
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_orders_with_currency_filter() {
    let filter = Filter::new();
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
    let mut events = Events::new(&filter);
    events.insert(e1);
    events.insert(e2);
    
    let out = parse_orders_events(
        events,
        Some("USD".into()),
        Some(Status::Active),
        None,
    );
    
    // Should only return USD orders
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_orders_no_filters() {
    let filter = Filter::new();
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
    let mut events = Events::new(&filter);
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
    let filter = Filter::new();
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
    
    let mut events = Events::new(&filter);
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
    let filter = Filter::new();
    let amounts = vec![10000i64, 50000i64, 100000i64, 1000000i64];
    let mut events = Events::new(&filter);
    
    for amount in &amounts {
        let e = build_order_event(
            mostro_core::order::Kind::Buy,
            Status::Active,
            "USD",
            *amount,
            (*amount / 100) as i64,
        );
        events.insert(e);
    }
    
    let out = parse_orders_events(events, Some("USD".into()), None, None);
    assert_eq!(out.len(), amounts.len());
}

#[test]
fn parse_orders_different_currencies() {
    let filter = Filter::new();
    let currencies = vec!["USD", "EUR", "GBP", "JPY", "CAD"];
    let mut events = Events::new(&filter);
    
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
    let filter = Filter::new();
    // Market price orders have amount = 0
    let e = build_order_event(
        mostro_core::order::Kind::Buy,
        Status::Active,
        "USD",
        0,
        1000,
    );
    let mut events = Events::new(&filter);
    events.insert(e);
    
    let out = parse_orders_events(events, Some("USD".into()), None, None);
    assert_eq!(out.len(), 1);
}