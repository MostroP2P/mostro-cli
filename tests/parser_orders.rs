use mostro_client::parser::orders::{parse_orders_events, print_orders_table};
use mostro_client::util::Event as ParsedEvent;
use mostro_core::order::Kind as OrderKind;
use mostro_core::prelude::*;
use nostr_sdk::prelude::Kind as EventKind;
use nostr_sdk::prelude::*;
use rstest::rstest;
use std::collections::BTreeSet;
use uuid::Uuid;

fn signed_order_event(created_at: u64, tags: impl IntoIterator<Item = Tag>) -> Event {
    EventBuilder::new(EventKind::Custom(38383), "")
        .tags(tags)
        .custom_created_at(Timestamp::from(created_at))
        .finalize(&Keys::generate())
        .unwrap()
}

fn build_order_event(
    kind: OrderKind,
    status: Status,
    fiat: &str,
    amount: i64,
    fiat_amount: i64,
) -> Event {
    let id = Uuid::new_v4();

    let mut tags = Tags::new();
    tags.push(Tag::custom("d", vec![id.to_string()]));
    tags.push(Tag::custom("k", vec![kind.to_string()]));
    tags.push(Tag::custom("f", vec![fiat.to_string()]));
    tags.push(Tag::custom("s", vec![status.to_string()]));
    tags.push(Tag::custom("amt", vec![amount.to_string()]));
    tags.push(Tag::custom("fa", vec![fiat_amount.to_string()]));

    signed_order_event(1_700_000_000, tags)
}

#[test]
fn latest_state_is_selected_before_status_filtering() {
    let order_id = Uuid::new_v4();
    let events: BTreeSet<_> = [
        (1_700_000_000, Status::Pending),
        (1_700_000_001, Status::Canceled),
    ]
    .into_iter()
    .map(|(created_at, status)| {
        signed_order_event(
            created_at,
            [
                Tag::custom("d", [order_id.to_string()]),
                Tag::custom("k", ["sell"]),
                Tag::custom("s", [status.to_string()]),
                Tag::custom("f", ["USD"]),
            ],
        )
    })
    .collect();

    let latest = parse_orders_events(events.clone(), None, None, None);
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].id, Some(order_id));
    assert_eq!(latest[0].status, Some(Status::Canceled));
    assert_eq!(latest[0].created_at, Some(1_700_000_001));
    assert!(parse_orders_events(events, None, Some(Status::Pending), None).is_empty());
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

fn listing_event(order_id: Uuid, created_at: u64, side: &str, status: &str, fiat: &str) -> Event {
    signed_order_event(
        created_at,
        [
            Tag::custom("d", [order_id.to_string()]),
            Tag::custom("k", [side]),
            Tag::custom("s", [status]),
            Tag::custom("f", [fiat]),
        ],
    )
}

#[rstest]
#[case("sell", "EUR")]
#[case("buy", "USD")]
fn latest_state_is_selected_before_currency_and_side_filters(
    #[case] side: &str,
    #[case] fiat: &str,
) {
    let order_id = Uuid::new_v4();
    let older = listing_event(order_id, 1_700_000_000, "sell", "pending", "USD");
    let newer = listing_event(order_id, 1_700_000_001, side, "pending", fiat);
    let events = BTreeSet::from([older, newer]);

    let result = parse_orders_events(
        events,
        Some("USD".to_string()),
        Some(Status::Pending),
        Some(OrderKind::Sell),
    );
    assert!(result.is_empty());
}

#[test]
fn duplicate_events_and_same_order_timestamp_ties_keep_lowest_event_id() {
    let order_id = Uuid::new_v4();
    let pending = listing_event(order_id, 1_700_000_000, "sell", "pending", "USD");
    let active = listing_event(order_id, 1_700_000_000, "sell", "active", "USD");
    let expected_status = if pending.id < active.id {
        Status::Pending
    } else {
        Status::Active
    };
    for events in [
        BTreeSet::from([pending.clone(), active.clone(), pending.clone()]),
        BTreeSet::from([active, pending]),
    ] {
        assert_eq!(events.len(), 2);
        let orders = parse_orders_events(events, None, None, None);
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, Some(order_id));
        assert_eq!(orders[0].status, Some(expected_status));
        assert_eq!(orders[0].created_at, Some(1_700_000_000));
    }
}

#[test]
fn rows_sort_newest_first_without_promising_cross_order_tie_order() {
    let newest_id = Uuid::new_v4();
    let tied_ids = [Uuid::new_v4(), Uuid::new_v4()];
    let events = BTreeSet::from([
        listing_event(tied_ids[0], 1_700_000_000, "sell", "pending", "USD"),
        listing_event(newest_id, 1_700_000_001, "sell", "pending", "USD"),
        listing_event(tied_ids[1], 1_700_000_000, "sell", "pending", "USD"),
    ]);
    let orders = parse_orders_events(events, None, None, None);

    assert_eq!(orders.len(), 3);
    assert_eq!(orders[0].id, Some(newest_id));
    assert_eq!(
        orders
            .iter()
            .map(|order| order.created_at)
            .collect::<Vec<_>>(),
        vec![
            Some(1_700_000_001),
            Some(1_700_000_000),
            Some(1_700_000_000)
        ]
    );
    assert_eq!(
        orders[1..]
            .iter()
            .map(|order| order.id.unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(tied_ids)
    );
}

#[rstest]
#[case(None, Some("sell"))]
#[case(Some("invalid-id"), Some("sell"))]
#[case(Some("00000000-0000-0000-0000-000000000001"), None)]
#[case(Some("00000000-0000-0000-0000-000000000001"), Some("invalid-side"))]
fn missing_or_invalid_identity_or_side_is_skipped(
    #[case] id: Option<&str>,
    #[case] side: Option<&str>,
) {
    let tags = [("d", id), ("k", side)]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| Tag::custom(name, [value])));
    let event = signed_order_event(1_700_000_000, tags);

    assert!(parse_orders_events(BTreeSet::from([event]), None, None, None).is_empty());
}

#[rstest]
#[case(None, None)]
#[case(Some("unknown"), Some(Status::Pending))]
#[case(Some(""), Some(Status::Pending))]
fn status_tag_absence_and_invalid_values_remain_distinct(
    #[case] status: Option<&str>,
    #[case] expected: Option<Status>,
) {
    let mut tags = vec![
        Tag::custom("d", [Uuid::new_v4().to_string()]),
        Tag::custom("k", ["sell"]),
    ];
    if let Some(status) = status {
        tags.push(Tag::custom("s", [status]));
    }
    let events = BTreeSet::from([signed_order_event(1_700_000_000, tags)]);
    let orders = parse_orders_events(events.clone(), None, None, None);

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status, expected);
    assert_eq!(orders[0].fiat_code, "");
    assert_eq!(orders[0].payment_method, "");
    assert_eq!(orders[0].amount, 0);
    assert_eq!(orders[0].fiat_amount, 0);
    assert_eq!(orders[0].premium, 0);
    assert_eq!(
        parse_orders_events(events, None, Some(Status::Pending), None).len(),
        usize::from(expected == Some(Status::Pending))
    );
}

#[rstest]
#[case(vec!["123"], 123, None, None, "123")]
#[case(vec!["invalid"], 0, None, None, "0")]
#[case(vec!["12.5"], 0, None, None, "0")]
#[case(vec!["100", "200"], 0, Some(100), Some(200), "100-200")]
#[case(vec!["100", "invalid"], 0, Some(100), None, "100-?")]
#[case(vec!["invalid", "200"], 0, None, Some(200), "?-200")]
#[case(vec!["200", "100"], 0, Some(200), Some(100), "200-100")]
#[case(vec!["12.5", "200"], 0, None, None, "0")]
fn permissive_numeric_tags_and_range_display(
    #[case] fiat_values: Vec<&str>,
    #[case] fixed: i64,
    #[case] minimum: Option<i64>,
    #[case] maximum: Option<i64>,
    #[case] displayed_fiat: &str,
) {
    let event = signed_order_event(
        1_700_000_000,
        [
            Tag::custom("d", [Uuid::new_v4().to_string()]),
            Tag::custom("k", ["sell"]),
            Tag::custom("s", ["pending"]),
            Tag::custom("amt", ["invalid"]),
            Tag::custom("fa", fiat_values),
            Tag::custom("premium", ["invalid"]),
        ],
    );
    let orders = parse_orders_events(BTreeSet::from([event]), None, None, None);
    assert_eq!(orders.len(), 1);
    let order = &orders[0];
    assert_eq!(order.amount, 0);
    assert_eq!(order.premium, 0);
    assert_eq!(
        (order.fiat_amount, order.min_amount, order.max_amount),
        (fixed, minimum, maximum)
    );

    let table =
        print_orders_table(orders.into_iter().map(ParsedEvent::SmallOrder).collect()).unwrap();
    let row = table.lines().find(|line| line.contains("market")).unwrap();
    let cells: Vec<_> = row.split(['\u{2502}', '\u{2506}']).map(str::trim).collect();
    assert_eq!(cells[4], "market");
    assert_eq!(cells[6], displayed_fiat);
}

#[test]
fn signed_i64_amounts_are_not_truncated_or_validated_as_positive() {
    let event = signed_order_event(
        1_700_000_000,
        [
            Tag::custom("d", [Uuid::new_v4().to_string()]),
            Tag::custom("k", ["sell"]),
            Tag::custom("amt", [i64::MIN.to_string()]),
            Tag::custom("fa", [i64::MAX.to_string()]),
            Tag::custom("premium", ["-7"]),
        ],
    );
    let orders = parse_orders_events(BTreeSet::from([event]), None, None, None);
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].amount, i64::MIN);
    assert_eq!(orders[0].fiat_amount, i64::MAX);
    assert_eq!(orders[0].premium, -7);
}

#[rstest]
#[case("USD", "USD", 1)]
#[case("USD", "usd", 0)]
#[case("USD", " USD", 0)]
#[case("", "", 1)]
#[case("USD", "", 0)]
fn reducer_currency_matching_is_exact(
    #[case] advertised: &str,
    #[case] requested: &str,
    #[case] count: usize,
) {
    let event = listing_event(Uuid::new_v4(), 1_700_000_000, "sell", "pending", advertised);
    let orders = parse_orders_events(
        BTreeSet::from([event]),
        Some(requested.to_string()),
        None,
        None,
    );
    assert_eq!(orders.len(), count);
}

#[test]
fn human_table_retains_order_business_fields_and_utc_event_time() {
    let order_id = Uuid::new_v4();
    let event = signed_order_event(
        1_700_000_000,
        [
            Tag::custom("d", [order_id.to_string()]),
            Tag::custom("k", ["sell"]),
            Tag::custom("s", ["pending"]),
            Tag::custom("amt", ["12345"]),
            Tag::custom("f", ["USD"]),
            Tag::custom("fa", ["123"]),
            Tag::custom("pm", ["wire", "cash"]),
            Tag::custom("premium", ["7"]),
        ],
    );
    let orders = parse_orders_events(BTreeSet::from([event]), None, None, None);
    assert_eq!(orders[0].payment_method, "wire,cash");
    let table =
        print_orders_table(orders.into_iter().map(ParsedEvent::SmallOrder).collect()).unwrap();
    let id_text = order_id.to_string();
    let row = table.lines().find(|line| line.contains(&id_text)).unwrap();
    let cells: Vec<_> = row.split(['\u{2502}', '\u{2506}']).map(str::trim).collect();
    assert!(cells[1].contains("sell"));
    assert_eq!(cells[2], id_text);
    assert!(cells[3].contains("pending"));
    assert_eq!(&cells[4..9], &["12345", "USD", "123", "wire,cash", "7"]);
    assert!(table.contains("2023-11-14"));
    assert!(table.contains("22:13"));
    for heading in [
        "Kind",
        "Order Id",
        "Status",
        "Amount",
        "Fiat",
        "Fiat Amt",
        "Payment Method",
        "Premium %",
        "Created",
    ] {
        assert!(table.contains(heading), "missing heading: {heading}");
    }
}
