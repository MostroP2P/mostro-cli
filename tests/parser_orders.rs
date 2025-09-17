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
