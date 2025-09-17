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
