use mostro_client::parser::disputes::{parse_dispute_events, print_disputes_table};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

fn build_dispute_event(id: uuid::Uuid, status: DisputeStatus) -> nostr_sdk::Event {
    let keys = Keys::generate();
    let mut tags = Tags::new();
    tags.push(Tag::custom(
        TagKind::Custom("d".into()),
        vec![id.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("y".into()),
        vec!["dispute".to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("s".into()),
        vec![status.to_string()],
    ));
    EventBuilder::new(nostr_sdk::Kind::TextNote, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .unwrap()
}

#[test]
fn parse_disputes_empty() {
    let filter = Filter::new();
    let events = Events::new(&filter);
    let out = parse_dispute_events(events);
    assert!(out.is_empty());
}

#[test]
fn parse_disputes_basic_and_print() {
    let filter = Filter::new();
    let id = uuid::Uuid::new_v4();
    let e = build_dispute_event(id, DisputeStatus::Initiated);
    let mut events = Events::new(&filter);
    events.insert(e);
    let out = parse_dispute_events(events);
    assert_eq!(out.len(), 1);

    let printable = out
        .into_iter()
        .map(mostro_client::util::Event::Dispute)
        .collect::<Vec<_>>();
    let table = print_disputes_table(printable).expect("table should render");
    assert!(table.contains(&id.to_string()));
}

#[test]
fn parse_disputes_multiple_statuses() {
    let filter = Filter::new();
    let statuses = vec![
        DisputeStatus::Initiated,
        DisputeStatus::InProgress,
        DisputeStatus::Settled,
        DisputeStatus::SellerRefunded,
    ];
    let mut events = Events::new(&filter);

    for status in &statuses {
        let id = uuid::Uuid::new_v4();
        let e = build_dispute_event(id, status.clone());
        events.insert(e);
    }

    let out = parse_dispute_events(events);
    assert_eq!(out.len(), statuses.len());
}

#[test]
fn print_disputes_empty_list() {
    let disputes: Vec<mostro_client::util::Event> = Vec::new();
    let table = print_disputes_table(disputes);

    assert!(table.is_ok());
    let table_str = table.unwrap();
    assert!(table_str.contains("No disputes found"));
}

#[test]
fn print_disputes_multiple_disputes() {
    let filter = Filter::new();
    let disputes = vec![
        build_dispute_event(uuid::Uuid::new_v4(), DisputeStatus::Initiated),
        build_dispute_event(uuid::Uuid::new_v4(), DisputeStatus::InProgress),
        build_dispute_event(uuid::Uuid::new_v4(), DisputeStatus::Settled),
    ];

    let mut events = Events::new(&filter);
    for dispute in disputes {
        events.insert(dispute);
    }

    let parsed = parse_dispute_events(events);
    let printable = parsed
        .into_iter()
        .map(mostro_client::util::Event::Dispute)
        .collect::<Vec<_>>();

    let table = print_disputes_table(printable);
    assert!(table.is_ok());

    let table_str = table.unwrap();
    assert!(!table_str.is_empty());
}

#[test]
fn parse_disputes_unique_ids() {
    let filter = Filter::new();
    let id1 = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();

    let e1 = build_dispute_event(id1, DisputeStatus::Initiated);
    let e2 = build_dispute_event(id2, DisputeStatus::Initiated);

    let mut events = Events::new(&filter);
    events.insert(e1);
    events.insert(e2);

    let out = parse_dispute_events(events);
    assert_eq!(out.len(), 2);

    assert_ne!(id1, id2);
}

#[test]
fn parse_disputes_initiated_status() {
    let filter = Filter::new();
    let id = uuid::Uuid::new_v4();
    let e = build_dispute_event(id, DisputeStatus::Initiated);

    let mut events = Events::new(&filter);
    events.insert(e);

    let out = parse_dispute_events(events);
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_disputes_settled_status() {
    let filter = Filter::new();
    let id = uuid::Uuid::new_v4();
    let e = build_dispute_event(id, DisputeStatus::Settled);

    let mut events = Events::new(&filter);
    events.insert(e);

    let out = parse_dispute_events(events);
    assert_eq!(out.len(), 1);
}

#[test]
fn parse_disputes_seller_refunded_status() {
    let filter = Filter::new();
    let id = uuid::Uuid::new_v4();
    let e = build_dispute_event(id, DisputeStatus::SellerRefunded);

    let mut events = Events::new(&filter);
    events.insert(e);

    let out = parse_dispute_events(events);
    assert_eq!(out.len(), 1);
}
