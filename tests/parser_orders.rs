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
    use mostro_client::outcome::order::{ListData, ListOrdersInput, SIDES, STATUSES};
    use mostro_client::outcome::ErrorCode;
    use mostro_client::parse_human::render_list;
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
            select_orders(&node(), filters, &events.into_iter().collect()).unwrap();
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
    fn cli_status_values_are_the_data_model_set() {
        assert_eq!(STATUSES, STATUS_SET);
        assert_eq!(SIDES, ["buy", "sell"]);
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

    /// Human text and JSON of one typed outcome.
    fn both_views(filters: ListOrdersInput, orders: &[SmallOrder]) -> (String, Value) {
        let (data, _) = ListData::from_selected(&node(), filters, orders).unwrap();
        (render_list(&data), serde_json::to_value(&data).unwrap())
    }

    /// Trimmed cells of the table line holding `id`; index 1 is the side.
    fn human_cells<'a>(text: &'a str, id: &str) -> Vec<&'a str> {
        let line = text
            .lines()
            .find(|line| line.contains(id))
            .unwrap_or_else(|| panic!("no row for {id}: {text}"));
        line.split(['\u{2502}', '\u{2506}'])
            .map(str::trim)
            .collect()
    }

    /// The human display the data model gives each JSON business field.
    fn expected_cells(order: &Value) -> [String; 8] {
        let text = |field: &str| order[field].as_str().unwrap().to_string();
        let sats = match text("sats_amount") {
            zero if zero == "0" => "market".to_string(),
            sats => sats,
        };
        let fiat = match (
            order["fiat_amount_min"].as_str(),
            order["fiat_amount_max"].as_str(),
        ) {
            (None, None) => text("fiat_amount"),
            (min, max) => format!("{}-{}", min.unwrap_or("?"), max.unwrap_or("?")),
        };
        let created = order["created_at"].as_str().map_or_else(
            || "Invalid date".to_string(),
            |at| at[..16].replace('T', " "),
        );
        [
            text("id"),
            text("status"),
            sats,
            text("fiat_currency"),
            fiat,
            text("payment_method"),
            text("premium_percent"),
            created,
        ]
    }

    #[test]
    fn human_rows_carry_the_json_business_fields() {
        let orders = [
            (1_700_000_000, 12_345, 50, None, None, -3, "sepa"),
            (1_700_000_100, 0, 0, Some(10), Some(20), 5, "wire,cash"),
            (-1, -5, -7, Some(-2), None, 0, ""),
            (253_402_300_799, 1, 1, None, Some(9), -1, "cash"),
            (253_402_300_800, 2, 2, None, None, 1, "btc"),
            (i64::MIN, 3, 3, None, None, 2, "eur"),
        ]
        .into_iter()
        .zip(1..)
        .map(
            |((at, sats, fiat, min, max, premium, payment), id)| SmallOrder {
                id: Some(Uuid::from_u128(id)),
                kind: Some(if id % 2 == 0 {
                    OrderKind::Buy
                } else {
                    OrderKind::Sell
                }),
                amount: sats,
                fiat_amount: fiat,
                min_amount: min,
                max_amount: max,
                premium,
                payment_method: payment.to_string(),
                ..selected(at)
            },
        )
        .collect::<Vec<_>>();
        let (text, json) = both_views(pending(), &orders);

        let rows = json["orders"].as_array().unwrap();
        assert_eq!(rows.len(), orders.len());
        for order in rows {
            let [id, status, rest @ ..] = expected_cells(order);
            let cells = human_cells(&text, &id);
            assert!(
                cells[1].contains(order["side"].as_str().unwrap()),
                "{cells:?}"
            );
            assert_eq!(cells[2], id);
            assert!(cells[3].contains(&status), "{cells:?}");
            assert_eq!(&cells[4..10], &rest, "{text}");
        }
    }

    #[test]
    fn human_heading_and_filter_summary_follow_the_typed_filters() {
        let (text, _) = both_views(pending(), &[selected(1_700_000_000)]);
        for expected in ["📋 List Orders", "Status Filter: pending", NODE_HEX] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(!text.contains("Order Type"), "{text}");
        assert!(!text.contains("Currency Filter"), "{text}");
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
            assert!(text.contains(heading), "missing heading {heading}: {text}");
        }

        let order = SmallOrder {
            status: Some(Status::Active),
            ..selected(1_700_000_000)
        };
        let (text, _) = both_views(filters(Some("SELL"), Some("usd"), Some("active")), &[order]);
        for expected in [
            "Status Filter: active",
            "Order Type: sell orders",
            "Currency Filter: USD",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn human_empty_book_keeps_the_existing_wording() {
        let (text, json) = both_views(filters(Some("buy"), Some("eur"), None), &[]);
        assert_eq!(json["orders"], json!([]));
        for expected in [
            "No Offers",
            "No offers found with requested parameters",
            "Order Type: buy orders",
            "Currency Filter: EUR",
            NODE_HEX,
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(!text.contains("Order Id"), "{text}");
    }

    #[rstest]
    #[case(1_700_000_000, "2023-11-14 22:13")]
    #[case(-1, "1969-12-31 23:59")]
    #[case(253_402_300_799, "9999-12-31 23:59")]
    #[case(-62_167_219_200, "0000-01-01 00:00")]
    #[case(253_402_300_800, "Invalid date")]
    #[case(-62_167_219_201, "Invalid date")]
    #[case(i64::MAX, "Invalid date")]
    #[case(i64::MIN, "Invalid date")]
    fn human_dates_are_the_utc_minute_or_invalid_date_like_json(
        #[case] seconds: i64,
        #[case] shown: &str,
    ) {
        let (text, json) = both_views(pending(), &[selected(seconds)]);
        let [id, .., created] = expected_cells(&json["orders"][0]);
        assert_eq!(created, shown);
        assert_eq!(human_cells(&text, &id)[9], shown, "{text}");
    }

    #[test]
    fn human_text_escapes_terminal_controls() {
        let order = SmallOrder {
            fiat_code: "U\r\u{7}".to_string(),
            payment_method: "\u{1b}[1m\n\u{9b}".to_string(),
            ..selected(1_700_000_000)
        };
        let (row_text, json) = both_views(pending(), &[order]);
        assert_eq!(json["orders"][0]["fiat_currency"], "U\r\u{7}");
        // Short values: a wrapped cell would split across table lines.
        let cells = human_cells(&row_text, "00000000-0000-0000-0000-000000000001");
        assert_eq!(cells[5], "U\\r\\u{7}", "{row_text}");
        assert_eq!(cells[7], "\\u{1b}[1m\\n\\u{9b}", "{row_text}");

        let (filter_text, _) = both_views(filters(None, Some("\u{1b}[2j"), None), &[]);
        assert!(
            filter_text.contains("Currency Filter: \\u{1b}[2J"),
            "{filter_text}"
        );

        for text in [row_text, filter_text] {
            for raw in ["\u{1b}[1m", "\u{1b}[2J", "\u{7}", "\u{9b}", "\r"] {
                assert!(!text.contains(raw), "raw {raw:?} reached the terminal");
            }
        }
    }
}

/// Wire facts the order-info fixtures rely on, checked against the pinned
/// `mostro-core`: the `orders` request and its replies survive the transport,
/// the identity proof matches the signing key, and the refusal reasons keep
/// their snake-case names. A divergence stops the order-info work.
mod info_contract {
    use mostro_core::order::{Kind as OrderKind, SmallOrder, Status as OrderStatus};
    use mostro_core::prelude::{
        unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, Payload, Transport,
        UnwrappedMessage, WrapOptions,
    };
    use nostr_sdk::prelude::Keys;
    use serde_json::json;
    use uuid::Uuid;

    async fn over_the_wire(message: &Message, signer: &Keys, receiver: &Keys) -> UnwrappedMessage {
        let event = wrap_message_with(
            Transport::Nip44Direct,
            message,
            signer,
            signer,
            receiver.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();
        unwrap_incoming(&event, receiver).await.unwrap().unwrap()
    }

    fn small_order(id: Uuid) -> SmallOrder {
        SmallOrder::new(
            Some(id),
            Some(OrderKind::Sell),
            Some(OrderStatus::Active),
            15_000,
            "USD".to_string(),
            None,
            None,
            15,
            "bank transfer".to_string(),
            1,
            Some(Keys::generate().public_key().to_hex()),
            None,
            None,
            Some(1_792_108_800),
            Some(1_792_195_200),
        )
    }

    #[tokio::test]
    async fn the_orders_request_carries_its_ids_and_the_identity_that_signed_it() {
        let identity = Keys::generate();
        let node = Keys::generate();
        let ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let request = Message::new_order(
            None,
            Some(8721),
            None,
            Action::Orders,
            Some(Payload::Ids(ids.clone())),
        );

        let seen = over_the_wire(&request, &identity, &node).await;

        assert_eq!(seen.sender, identity.public_key());
        assert_eq!(seen.identity, identity.public_key());
        let kind = seen.message.get_inner_message_kind();
        assert_eq!(kind.request_id, Some(8721));
        assert_eq!(kind.action, Action::Orders);
        let Some(Payload::Ids(received)) = &kind.payload else {
            panic!("unexpected payload {:?}", kind.payload);
        };
        assert_eq!(received, &ids);
    }

    #[tokio::test]
    async fn the_orders_reply_keeps_request_id_orders_and_trade_keys() {
        let identity = Keys::generate();
        let node = Keys::generate();
        let id = Uuid::new_v4();
        let order = small_order(id);
        let reply = Message::new_order(
            None,
            Some(8721),
            None,
            Action::Orders,
            Some(Payload::Orders(vec![order.clone()])),
        );

        let seen = over_the_wire(&reply, &node, &identity).await;

        assert_eq!(seen.sender, node.public_key());
        let kind = seen.message.get_inner_message_kind();
        assert_eq!(
            (kind.request_id, kind.action.clone()),
            (Some(8721), Action::Orders)
        );
        let Some(Payload::Orders(orders)) = &kind.payload else {
            panic!("unexpected payload {:?}", kind.payload);
        };
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, Some(id));
        assert_eq!(orders[0].buyer_trade_pubkey, order.buyer_trade_pubkey);
        assert_eq!(orders[0].buyer_invoice, None);
        assert_eq!(orders[0].created_at, Some(1_792_108_800));
    }

    #[tokio::test]
    async fn refusals_are_cant_do_messages_with_snake_case_reasons() {
        let identity = Keys::generate();
        let node = Keys::generate();
        let cases = [
            (CantDoReason::NotFound, "not_found"),
            (CantDoReason::TooManyRequests, "too_many_requests"),
            (CantDoReason::InvalidParameters, "invalid_parameters"),
        ];
        for (reason, name) in cases {
            assert_eq!(serde_json::to_value(&reason).unwrap(), json!(name));
            let refusal = Message::cant_do(
                None,
                Some(8721),
                Some(Payload::CantDo(Some(reason.clone()))),
            );
            let seen = over_the_wire(&refusal, &node, &identity).await;
            let kind = seen.message.get_inner_message_kind();
            assert_eq!(
                (kind.request_id, kind.action.clone()),
                (Some(8721), Action::CantDo)
            );
            let Some(Payload::CantDo(Some(received))) = &kind.payload else {
                panic!("{name}: unexpected payload {:?}", kind.payload);
            };
            assert_eq!(received, &reason, "{name}");
        }
    }

    // --- The typed data of `order.info` (spec 002, slice 2) -----------------

    use mostro_client::outcome::info::{OrderInfoData, OrderInfoInput};
    use mostro_client::outcome::{WarningCode, Warnings};
    use nostr_sdk::prelude::{PublicKey, ToBech32};
    use serde_json::Value;

    fn node() -> PublicKey {
        Keys::generate().public_key()
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn order_with(n: u128, edit: impl FnOnce(&mut SmallOrder)) -> SmallOrder {
        let mut order = small_order(id(n));
        edit(&mut order);
        order
    }

    fn input(ids: &[u128]) -> OrderInfoInput {
        OrderInfoInput::new(ids.iter().map(|n| id(*n))).expect("at least one id")
    }

    fn wire(data: &OrderInfoData) -> Value {
        serde_json::to_value(data).unwrap()
    }

    fn ids_of(value: &Value, key: &str) -> Vec<String> {
        value[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.get("id").unwrap_or(item).as_str().unwrap().to_string())
            .collect()
    }

    fn text(n: u128) -> String {
        id(n).to_string()
    }

    #[test]
    fn the_input_needs_an_id_and_keeps_each_once_in_order() {
        assert!(OrderInfoInput::new(Vec::<Uuid>::new()).is_none());
        let repeated = OrderInfoInput::new([id(3), id(1), id(3), id(2), id(1)]).unwrap();
        assert_eq!(repeated.ids(), [id(3), id(1), id(2)]);
    }

    #[test]
    fn returned_orders_follow_the_request_each_once_and_only_when_requested() {
        // The node answers in its own order, adds an order nobody asked for and
        // repeats one.
        let reply = vec![
            order_with(2, |_| {}),
            order_with(9, |_| {}),
            order_with(1, |_| {}),
            order_with(2, |order| order.fiat_amount = 99),
        ];

        let (data, warnings) = OrderInfoData::from_reply(&node(), &input(&[1, 2, 3]), &reply);

        let value = wire(&data);
        assert_eq!(ids_of(&value, "orders"), [text(1), text(2)]);
        assert_eq!(ids_of(&value, "not_returned"), [text(3)]);
        // The first copy of a repeated order wins.
        assert_eq!(value["orders"][1]["fiat_amount"], json!("15"));
        assert!(warnings.contains(WarningCode::OrdersNotReturned));
    }

    #[test]
    fn an_empty_reply_reports_every_id_as_not_returned() {
        let (data, warnings) = OrderInfoData::from_reply(&node(), &input(&[5, 4]), &[]);

        let value = wire(&data);
        assert_eq!(value["orders"], json!([]));
        assert_eq!(ids_of(&value, "not_returned"), [text(5), text(4)]);
        assert!(warnings.contains(WarningCode::OrdersNotReturned));
    }

    #[test]
    fn a_complete_reply_has_no_warning_and_no_unreturned_ids() {
        let reply = vec![order_with(1, |_| {}), order_with(2, |_| {})];

        let (data, warnings) = OrderInfoData::from_reply(&node(), &input(&[1, 2]), &reply);

        assert_eq!(wire(&data)["not_returned"], json!([]));
        assert!(warnings.is_empty());
    }

    #[test]
    fn every_requested_id_is_returned_or_not_returned_exactly_once() {
        let requested = [1u128, 2, 3, 4, 5, 6];
        for mask in 0u32..(1 << requested.len()) {
            let reply: Vec<SmallOrder> = requested
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, n)| order_with(*n, |_| {}))
                .chain([order_with(99, |_| {})])
                .collect();
            let (data, _) = OrderInfoData::from_reply(&node(), &input(&requested), &reply);
            let value = wire(&data);
            let mut seen: Vec<String> = ids_of(&value, "orders");
            seen.extend(ids_of(&value, "not_returned"));
            seen.sort();
            let mut expected: Vec<String> = requested.iter().map(|n| text(*n)).collect();
            expected.sort();
            assert_eq!(seen, expected, "mask {mask:b}");
        }
    }

    #[test]
    fn records_use_explicit_canonical_fields_and_never_the_buyer_invoice() {
        let reply = vec![order_with(1, |order| {
            order.amount = i64::MIN;
            order.premium = -5;
            order.min_amount = Some(10);
            order.max_amount = Some(20);
            order.buyer_invoice = Some("lnbc1secretinvoice".to_string());
        })];

        let (data, _) = OrderInfoData::from_reply(&node(), &input(&[1]), &reply);

        let value = wire(&data);
        let record = &value["orders"][0];
        let mut keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "buyer_trade_pubkey",
                "created_at",
                "created_at_unix",
                "expires_at",
                "expires_at_unix",
                "fiat_amount",
                "fiat_amount_max",
                "fiat_amount_min",
                "fiat_currency",
                "id",
                "payment_method",
                "premium_percent",
                "sats_amount",
                "seller_trade_pubkey",
                "side",
                "status",
            ]
        );
        assert_eq!(record["sats_amount"], json!("-9223372036854775808"));
        assert_eq!(record["premium_percent"], json!("-5"));
        assert_eq!(record["fiat_amount_min"], json!("10"));
        assert_eq!(record["fiat_amount_max"], json!("20"));
        assert_eq!(
            (record["side"].as_str(), record["status"].as_str()),
            (Some("sell"), Some("active"))
        );
        assert!(!value.to_string().contains("secretinvoice"));
        let node_text = value["node_pubkey"].as_str().unwrap();
        assert_eq!(node_text.len(), 64);
        assert_eq!(node_text, node_text.to_lowercase());
    }

    #[test]
    fn missing_optional_values_are_null_and_never_invented() {
        let reply = vec![order_with(1, |order| {
            order.kind = None;
            order.status = None;
            order.min_amount = None;
            order.max_amount = None;
            order.created_at = None;
            order.expires_at = None;
        })];

        let (data, warnings) = OrderInfoData::from_reply(&node(), &input(&[1]), &reply);

        let record = &wire(&data)["orders"][0];
        for key in [
            "side",
            "status",
            "fiat_amount_min",
            "fiat_amount_max",
            "created_at",
            "created_at_unix",
            "expires_at",
            "expires_at_unix",
        ] {
            assert_eq!(record[key], Value::Null, "{key}");
        }
        assert!(warnings.is_empty());
    }

    #[test]
    fn trade_keys_are_lowercase_hex_or_null() {
        let key = Keys::generate().public_key();
        let cases = [
            (Some(key.to_hex().to_uppercase()), json!(key.to_hex())),
            (Some(key.to_bech32().unwrap()), json!(key.to_hex())),
            (Some("not a key".to_string()), Value::Null),
            (Some(String::new()), Value::Null),
            (None, Value::Null),
        ];
        for (given, expected) in cases {
            let reply = vec![order_with(1, |order| {
                order.buyer_trade_pubkey = given.clone();
                order.seller_trade_pubkey = given.clone();
            })];
            let (data, _) = OrderInfoData::from_reply(&node(), &input(&[1]), &reply);
            let record = &wire(&data)["orders"][0];
            assert_eq!(record["buyer_trade_pubkey"], expected, "{given:?}");
            assert_eq!(record["seller_trade_pubkey"], expected, "{given:?}");
        }
    }

    #[test]
    fn times_are_utc_text_plus_the_exact_integer_and_null_when_unrepresentable() {
        let reply = vec![order_with(1, |order| {
            order.created_at = Some(1_792_108_800);
            order.expires_at = Some(i64::MAX);
        })];

        let (data, warnings) = OrderInfoData::from_reply(&node(), &input(&[1]), &reply);

        let record = &wire(&data)["orders"][0];
        assert_eq!(record["created_at"], json!("2026-10-16T00:00:00Z"));
        assert_eq!(record["created_at_unix"], json!("1792108800"));
        assert_eq!(record["expires_at"], Value::Null);
        assert_eq!(record["expires_at_unix"], json!("9223372036854775807"));
        assert!(warnings.contains(WarningCode::InvalidOrderTimestamp));
        assert!(!warnings.contains(WarningCode::OrdersNotReturned));
    }

    #[test]
    fn warnings_are_aggregated_once_per_code() {
        let reply = vec![
            order_with(1, |order| order.created_at = Some(i64::MIN)),
            order_with(2, |order| order.expires_at = Some(i64::MAX)),
        ];
        let (_, warnings) = OrderInfoData::from_reply(&node(), &input(&[1, 2, 3, 4]), &reply);
        let mut expected = Warnings::new();
        expected.add(WarningCode::OrdersNotReturned);
        expected.add(WarningCode::InvalidOrderTimestamp);
        let wire = |warnings: &Warnings| {
            serde_json::to_value(warnings)
                .unwrap()
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(wire(&warnings), wire(&expected));
        assert!(warnings.contains(WarningCode::OrdersNotReturned));
        assert!(warnings.contains(WarningCode::InvalidOrderTimestamp));
    }
}
