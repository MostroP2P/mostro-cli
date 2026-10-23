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

#[test]
fn event_times_beyond_i64_saturate_instead_of_wrapping() {
    let order_id = Uuid::new_v4();
    let older = listing_event(order_id, 1_700_000_000, "sell", "pending", "USD");
    let far_future = listing_event(order_id, u64::MAX, "sell", "canceled", "USD");
    let events = BTreeSet::from([older, far_future]);

    let latest = parse_orders_events(events.clone(), None, None, None);
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].status, Some(Status::Canceled));
    assert_eq!(latest[0].created_at, Some(i64::MAX));
    assert!(parse_orders_events(events, None, Some(Status::Pending), None).is_empty());

    let table =
        print_orders_table(latest.into_iter().map(ParsedEvent::SmallOrder).collect()).unwrap();
    assert!(table.contains("Invalid date"));
    assert!(!table.contains("1969"));
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

/// `order.list` DTO and selection contract from
/// `specs/001-order-list-json/data-model.md`. The examples file is a proposed
/// contract, not captured runtime output. Each test is ignored until the task
/// in its reason implements the behavior; that task removes the attribute.
mod list_contract {
    use super::*;
    use mostro_client::commands::order::list::select_orders;
    use mostro_client::outcome::order::{ListData, ListOrdersInput};
    use mostro_client::outcome::ErrorCode;
    use serde_json::{json, Value};

    const NODE_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const EXAMPLES: &str = include_str!("../specs/001-order-list-json/contracts/examples.json");

    /// The data-model status set, in its documented order.
    const STATUS_SET: [&str; 17] = [
        "active",
        "canceled",
        "canceled-by-admin",
        "settled-by-admin",
        "completed-by-admin",
        "dispute",
        "expired",
        "fiat-sent",
        "settled-hold-invoice",
        "pending",
        "success",
        "waiting-buyer-invoice",
        "waiting-payment",
        "waiting-taker-bond",
        "waiting-maker-bond",
        "cooperatively-canceled",
        "in-progress",
    ];

    /// No wildcard arm: a status added to the pinned core fails to compile
    /// here until the contract decides its wire name.
    fn wire_name(status: Status) -> &'static str {
        match status {
            Status::Active => "active",
            Status::Canceled => "canceled",
            Status::CanceledByAdmin => "canceled-by-admin",
            Status::SettledByAdmin => "settled-by-admin",
            Status::CompletedByAdmin => "completed-by-admin",
            Status::Dispute => "dispute",
            Status::Expired => "expired",
            Status::FiatSent => "fiat-sent",
            Status::SettledHoldInvoice => "settled-hold-invoice",
            Status::Pending => "pending",
            Status::Success => "success",
            Status::WaitingBuyerInvoice => "waiting-buyer-invoice",
            Status::WaitingPayment => "waiting-payment",
            Status::WaitingTakerBond => "waiting-taker-bond",
            Status::WaitingMakerBond => "waiting-maker-bond",
            Status::CooperativelyCanceled => "cooperatively-canceled",
            Status::InProgress => "in-progress",
        }
    }

    fn node() -> PublicKey {
        PublicKey::from_hex(NODE_HEX).unwrap()
    }

    fn filters(
        side: Option<&str>,
        currency: Option<&str>,
        status: Option<&str>,
    ) -> ListOrdersInput {
        ListOrdersInput::parse(side, currency, status).unwrap()
    }

    fn pending() -> ListOrdersInput {
        filters(None, None, None)
    }

    fn selected(created_at: i64) -> SmallOrder {
        SmallOrder {
            id: Some(Uuid::from_u128(1)),
            kind: Some(OrderKind::Sell),
            status: Some(Status::Pending),
            fiat_code: "USD".to_string(),
            created_at: Some(created_at),
            ..SmallOrder::default()
        }
    }

    fn mapped(filters: ListOrdersInput, orders: &[SmallOrder]) -> (Value, Value) {
        let (data, warnings) = ListData::from_selected(&node(), filters, orders).unwrap();
        (
            serde_json::to_value(data).unwrap(),
            serde_json::to_value(warnings).unwrap(),
        )
    }

    fn row(order: SmallOrder) -> Value {
        mapped(pending(), &[order]).0["orders"][0].clone()
    }

    fn selection(
        filters: ListOrdersInput,
        events: impl IntoIterator<Item = Event>,
    ) -> (Value, Value) {
        let (data, warnings) =
            select_orders(&node(), filters, events.into_iter().collect()).unwrap();
        (
            serde_json::to_value(data).unwrap(),
            serde_json::to_value(warnings).unwrap(),
        )
    }

    fn ids(data: &Value) -> Vec<&str> {
        data["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|order| order["id"].as_str().unwrap())
            .collect()
    }

    fn timestamp_warnings(warnings: &Value) -> usize {
        warnings
            .as_array()
            .unwrap()
            .iter()
            .filter(|warning| warning["code"] == "invalid_event_timestamp")
            .count()
    }

    fn example(name: &str) -> Value {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        examples["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()["stdout"]
            .clone()
    }

    #[test]
    fn default_filters_are_pending_with_null_side_and_currency() {
        assert_eq!(
            serde_json::to_value(pending()).unwrap(),
            json!({ "side": null, "currency": null, "status": "pending" })
        );
    }

    #[rstest]
    #[case(Some("sell"), None, json!("sell"), json!("pending"))]
    #[case(Some("SELL"), None, json!("sell"), json!("pending"))]
    #[case(Some("Buy"), None, json!("buy"), json!("pending"))]
    #[case(None, Some("PENDING"), json!(null), json!("pending"))]
    #[case(None, Some("Canceled-By-Admin"), json!(null), json!("canceled-by-admin"))]
    fn side_and_status_are_case_insensitive(
        #[case] side: Option<&str>,
        #[case] status: Option<&str>,
        #[case] wire_side: Value,
        #[case] wire_status: Value,
    ) {
        let input = serde_json::to_value(filters(side, None, status)).unwrap();
        assert_eq!(
            (&input["side"], &input["status"]),
            (&wire_side, &wire_status)
        );
    }

    #[rstest]
    #[case(Some(" sell"), None)]
    #[case(Some("sell "), None)]
    #[case(Some(""), None)]
    #[case(Some("both"), None)]
    #[case(None, Some(" pending"))]
    #[case(None, Some("pending\n"))]
    #[case(None, Some(""))]
    #[case(None, Some("all"))]
    fn untrimmed_or_unknown_side_and_status_are_invalid_arguments(
        #[case] side: Option<&str>,
        #[case] status: Option<&str>,
    ) {
        assert_eq!(
            ListOrdersInput::parse(side, None, status),
            Err(ErrorCode::InvalidArguments)
        );
    }

    #[rstest]
    #[case("usd", "USD")]
    #[case("Eur", "EUR")]
    #[case(" usd", " USD")]
    #[case("", "")]
    fn currency_is_uppercased_without_trimming_or_nulling(
        #[case] requested: &str,
        #[case] normalized: &str,
    ) {
        let input = filters(None, Some(requested), None);
        assert_eq!(input.currency(), Some(normalized));
        assert_eq!(serde_json::to_value(input).unwrap()["currency"], normalized);
    }

    #[test]
    fn every_core_status_is_a_canonical_filter_and_wire_value() {
        for name in STATUS_SET {
            for spelling in [name.to_string(), name.to_uppercase()] {
                let input = filters(None, None, Some(&spelling));
                assert_eq!(wire_name(input.status()), name);
                assert_eq!(serde_json::to_value(&input).unwrap()["status"], name);
            }
            let status: Status = name.parse().unwrap();
            let order = SmallOrder {
                status: Some(status),
                ..selected(1_700_000_000)
            };
            let (data, _) = mapped(filters(None, None, Some(name)), &[order]);
            assert_eq!(data["orders"][0]["status"], name);
        }
    }

    #[test]
    fn list_data_carries_node_hex_filters_and_canonical_identity() {
        let order = SmallOrder {
            id: Some(Uuid::parse_str("00000000-0000-4000-8000-00000000000A").unwrap()),
            ..selected(1_700_000_000)
        };
        let (data, warnings) = mapped(pending(), &[order]);
        assert_eq!(data["node_pubkey"], NODE_HEX);
        assert_eq!(data["filters"], serde_json::to_value(pending()).unwrap());
        assert_eq!(
            data["orders"][0]["id"],
            "00000000-0000-4000-8000-00000000000a"
        );
        assert_eq!(data["orders"][0]["side"], "sell");
        assert_eq!(data["orders"][0]["status"], "pending");
        assert_eq!(warnings, json!([]));

        let (empty, warnings) = mapped(pending(), &[]);
        assert_eq!(empty["orders"], json!([]));
        assert_eq!(warnings, json!([]));
    }

    #[test]
    fn rows_keep_the_reducer_order() {
        let newer = SmallOrder {
            id: Some(Uuid::from_u128(2)),
            ..selected(1_700_000_001)
        };
        let (data, _) = mapped(pending(), &[newer, selected(1_700_000_000)]);
        assert_eq!(
            ids(&data),
            [
                "00000000-0000-0000-0000-000000000002",
                "00000000-0000-0000-0000-000000000001"
            ]
        );
    }

    #[rstest]
    #[case(pending(), SmallOrder { id: None, ..selected(1_700_000_000) })]
    #[case(pending(), SmallOrder { kind: None, ..selected(1_700_000_000) })]
    #[case(pending(), SmallOrder { status: None, ..selected(1_700_000_000) })]
    #[case(pending(), SmallOrder { created_at: None, ..selected(1_700_000_000) })]
    #[case(pending(), SmallOrder { status: Some(Status::Active), ..selected(1_700_000_000) })]
    #[case(filters(Some("buy"), None, None), selected(1_700_000_000))]
    #[case(filters(None, Some("eur"), None), selected(1_700_000_000))]
    fn rows_breaking_the_selection_invariants_are_internal_errors(
        #[case] filters: ListOrdersInput,
        #[case] order: SmallOrder,
    ) {
        assert_eq!(
            ListData::from_selected(&node(), filters, &[order]).map(|_| ()),
            Err(ErrorCode::InternalError)
        );
    }

    #[rstest]
    #[case(i64::MIN, "-9223372036854775808")]
    #[case(i64::MAX, "9223372036854775807")]
    #[case(-5, "-5")]
    #[case(0, "0")]
    #[case(15_000, "15000")]
    fn money_fields_are_canonical_signed_decimal_strings(
        #[case] value: i64,
        #[case] decimal: &str,
    ) {
        let order = row(SmallOrder {
            amount: value,
            fiat_amount: value,
            premium: value,
            min_amount: Some(value),
            max_amount: Some(value),
            ..selected(1_700_000_000)
        });
        for field in [
            "sats_amount",
            "fiat_amount",
            "premium_percent",
            "fiat_amount_min",
            "fiat_amount_max",
        ] {
            assert_eq!(order[field], decimal, "{field}");
        }
    }

    #[test]
    fn zero_sats_stays_the_market_priced_zero() {
        assert_eq!(row(selected(1_700_000_000))["sats_amount"], "0");
    }

    #[rstest]
    #[case(None, None, json!(null), json!(null))]
    #[case(Some(100), None, json!("100"), json!(null))]
    #[case(None, Some(200), json!(null), json!("200"))]
    #[case(Some(200), Some(100), json!("200"), json!("100"))]
    fn incomplete_or_unordered_ranges_stay_as_decoded(
        #[case] minimum: Option<i64>,
        #[case] maximum: Option<i64>,
        #[case] wire_min: Value,
        #[case] wire_max: Value,
    ) {
        let order = row(SmallOrder {
            fiat_amount: 7,
            min_amount: minimum,
            max_amount: maximum,
            ..selected(1_700_000_000)
        });
        assert_eq!(order["fiat_amount"], "7");
        assert_eq!(
            (&order["fiat_amount_min"], &order["fiat_amount_max"]),
            (&wire_min, &wire_max)
        );
    }

    #[test]
    fn public_text_is_kept_as_received() {
        for (fiat, payment) in [("", ""), ("usd", "wire,cash"), ("USD", "\u{1b}[31mred")] {
            let order = row(SmallOrder {
                fiat_code: fiat.to_string(),
                payment_method: payment.to_string(),
                ..selected(1_700_000_000)
            });
            assert_eq!(order["fiat_currency"], fiat);
            assert_eq!(order["payment_method"], payment);
        }
    }

    #[rstest]
    #[case(1_700_000_000, Some("2023-11-14T22:13:20Z"))]
    #[case(0, Some("1970-01-01T00:00:00Z"))]
    #[case(-1, Some("1969-12-31T23:59:59Z"))]
    #[case(253_402_300_799, Some("9999-12-31T23:59:59Z"))]
    #[case(253_402_300_800, None)]
    #[case(-62_167_219_200, Some("0000-01-01T00:00:00Z"))]
    #[case(-62_167_219_201, None)]
    #[case(i64::MAX, None)]
    #[case(i64::MIN, None)]
    fn event_times_are_utc_rfc3339_within_four_digit_years(
        #[case] seconds: i64,
        #[case] rfc3339: Option<&str>,
    ) {
        let (data, warnings) = mapped(pending(), &[selected(seconds)]);
        let order = &data["orders"][0];
        assert_eq!(order["created_at"], json!(rfc3339));
        assert_eq!(order["created_at_unix"], seconds.to_string());
        assert_eq!(
            timestamp_warnings(&warnings),
            usize::from(rfc3339.is_none())
        );
        assert_eq!(
            warnings.as_array().unwrap().len(),
            usize::from(rfc3339.is_none())
        );
    }

    #[test]
    fn invalid_event_times_share_one_warning_and_keep_their_rows() {
        let orders: Vec<_> = [i64::MAX, 1_700_000_000, i64::MIN]
            .into_iter()
            .zip(1..)
            .map(|(seconds, id)| SmallOrder {
                id: Some(Uuid::from_u128(id)),
                ..selected(seconds)
            })
            .collect();
        let (data, warnings) = mapped(pending(), &orders);
        assert_eq!(data["orders"].as_array().unwrap().len(), 3);
        assert_eq!(timestamp_warnings(&warnings), 1);
        assert_eq!(warnings.as_array().unwrap().len(), 1);
    }

    #[test]
    fn proposed_examples_are_reproduced_exactly() {
        let order = SmallOrder {
            id: Some(Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap()),
            amount: 9_007_199_254_740_993,
            fiat_amount: 15_000,
            payment_method: "synthetic bank transfer".to_string(),
            premium: -5,
            ..selected(1_792_108_800)
        };
        let (data, warnings) = mapped(filters(Some("sell"), Some("usd"), None), &[order]);
        let large = example("large_amount_success");
        assert_eq!(data, large["data"]);
        assert_eq!(warnings, large["warnings"]);

        let (data, warnings) = mapped(pending(), &[]);
        let empty = example("empty_success");
        assert_eq!(data, empty["data"]);
        assert_eq!(warnings, empty["warnings"]);
    }

    #[test]
    #[ignore = "red until T028"]
    fn selection_reduces_to_the_latest_state_before_filtering() {
        let order_id = Uuid::from_u128(1);
        let stale_pending = [
            listing_event(order_id, 1_700_000_000, "sell", "pending", "USD"),
            listing_event(order_id, 1_700_000_001, "sell", "canceled", "USD"),
        ];
        assert_eq!(selection(pending(), stale_pending).0["orders"], json!([]));

        let reopened = [
            listing_event(order_id, 1_700_000_000, "sell", "canceled", "USD"),
            listing_event(order_id, 1_700_000_001, "sell", "pending", "USD"),
        ];
        let (data, _) = selection(pending(), reopened);
        assert_eq!(ids(&data), ["00000000-0000-0000-0000-000000000001"]);
        assert_eq!(data["orders"][0]["created_at_unix"], "1700000001");
    }

    #[test]
    #[ignore = "red until T028"]
    fn same_order_time_ties_keep_the_lowest_event_id() {
        let order_id = Uuid::from_u128(1);
        let pending_event = listing_event(order_id, 1_700_000_000, "sell", "pending", "USD");
        let active_event = listing_event(order_id, 1_700_000_000, "sell", "active", "USD");
        let winner = if pending_event.id < active_event.id {
            "pending"
        } else {
            "active"
        };
        for status in ["pending", "active"] {
            let (data, _) = selection(
                filters(None, None, Some(status)),
                [pending_event.clone(), active_event.clone()],
            );
            assert_eq!(
                data["orders"].as_array().unwrap().len(),
                usize::from(status == winner)
            );
        }
    }

    #[test]
    #[ignore = "red until T028"]
    fn cross_order_time_ties_have_no_promised_order() {
        let events = [
            listing_event(Uuid::from_u128(1), 1_700_000_000, "sell", "pending", "USD"),
            listing_event(Uuid::from_u128(2), 1_700_000_001, "sell", "pending", "USD"),
            listing_event(Uuid::from_u128(3), 1_700_000_000, "sell", "pending", "USD"),
        ];
        let (data, _) = selection(pending(), events);
        let rows = ids(&data);
        assert_eq!(rows[0], "00000000-0000-0000-0000-000000000002");
        assert_eq!(
            rows[1..].iter().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "00000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000003"
            ])
        );
    }

    #[test]
    #[ignore = "red until T028"]
    fn absent_status_never_matches_and_invalid_status_is_pending() {
        let absent = signed_order_event(
            1_700_000_000,
            [
                Tag::custom("d", [Uuid::from_u128(1).to_string()]),
                Tag::custom("k", ["sell"]),
            ],
        );
        for status in STATUS_SET {
            let (data, _) = selection(filters(None, None, Some(status)), [absent.clone()]);
            assert_eq!(data["orders"], json!([]), "{status}");
        }

        let invalid = listing_event(Uuid::from_u128(2), 1_700_000_000, "sell", "unknown", "USD");
        let (data, _) = selection(pending(), [invalid]);
        assert_eq!(data["orders"][0]["status"], "pending");
    }

    #[test]
    #[ignore = "red until T028"]
    fn malformed_events_are_skipped_without_failing_the_read() {
        let bad_id = signed_order_event(
            1_700_000_000,
            [
                Tag::custom("d", ["not-a-uuid"]),
                Tag::custom("k", ["sell"]),
                Tag::custom("s", ["pending"]),
            ],
        );
        let bad_side = listing_event(Uuid::from_u128(2), 1_700_000_000, "both", "pending", "USD");
        let valid = listing_event(Uuid::from_u128(3), 1_700_000_000, "sell", "pending", "USD");
        let (data, warnings) = selection(pending(), [bad_id, bad_side, valid]);
        assert_eq!(ids(&data), ["00000000-0000-0000-0000-000000000003"]);
        assert_eq!(warnings, json!([]));
    }

    #[test]
    #[ignore = "red until T028"]
    fn selected_ids_use_the_canonical_form() {
        let event = signed_order_event(
            1_700_000_000,
            [
                Tag::custom("d", ["0000000000004000800000000000000A"]),
                Tag::custom("k", ["sell"]),
                Tag::custom("s", ["pending"]),
            ],
        );
        let (data, _) = selection(pending(), [event]);
        assert_eq!(ids(&data), ["00000000-0000-4000-8000-00000000000a"]);
    }

    #[test]
    #[ignore = "red until T028"]
    fn far_future_events_saturate_with_one_warning() {
        let events = [
            listing_event(Uuid::from_u128(1), u64::MAX, "sell", "pending", "USD"),
            listing_event(Uuid::from_u128(2), 1_700_000_000, "sell", "pending", "USD"),
        ];
        let (data, warnings) = selection(pending(), events);
        assert_eq!(data["orders"][0]["created_at"], Value::Null);
        assert_eq!(data["orders"][0]["created_at_unix"], i64::MAX.to_string());
        assert_eq!(data["orders"][1]["created_at"], "2023-11-14T22:13:20Z");
        assert_eq!(timestamp_warnings(&warnings), 1);
    }

    #[test]
    #[ignore = "red until T028"]
    fn currency_filter_matches_exactly_after_normalization() {
        let events = [
            listing_event(Uuid::from_u128(1), 1_700_000_000, "sell", "pending", "USD"),
            listing_event(Uuid::from_u128(2), 1_700_000_000, "sell", "pending", "usd"),
            listing_event(Uuid::from_u128(3), 1_700_000_000, "buy", "pending", "USD"),
        ];
        let (data, _) = selection(filters(Some("SELL"), Some("usd"), None), events);
        assert_eq!(ids(&data), ["00000000-0000-0000-0000-000000000001"]);
        assert_eq!(
            data["filters"],
            json!({ "side": "sell", "currency": "USD", "status": "pending" })
        );
    }
}
