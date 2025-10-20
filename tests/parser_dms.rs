use mostro_client::parser::dms::{parse_dm_events, print_direct_messages};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

#[tokio::test]
async fn parse_dm_empty() {
    let keys = Keys::generate();
    let events = Events::new(&Filter::new());
    let out = parse_dm_events(events, &keys, None).await;
    assert!(out.is_empty());
}

#[tokio::test]
async fn print_dms_empty() {
    let msgs: Vec<(Message, u64, PublicKey)> = Vec::new();
    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_mostro_pubkey() {
    let mostro_key = Keys::generate();
    let msgs: Vec<(Message, u64, PublicKey)> = Vec::new();
    let res = print_direct_messages(&msgs, Some(mostro_key.public_key())).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_single_message() {
    let sender_keys = Keys::generate();
    let message = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::NewOrder,
        None,
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_text_payload() {
    let sender_keys = Keys::generate();
    let text_payload = Payload::TextMessage("Hello World".to_string());
    let message = Message::new_dm(None, None, Action::SendDm, Some(text_payload));
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_payment_request() {
    let sender_keys = Keys::generate();
    let invoice = "lnbc1000n1...".to_string();
    let payment_payload = Payload::PaymentRequest(None, invoice.clone(), None);
    let message = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::PayInvoice,
        Some(payment_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_multiple_messages() {
    let sender_keys = Keys::generate();
    let mut msgs = Vec::new();

    let actions = [
        Action::NewOrder,
        Action::PayInvoice,
        Action::FiatSent,
        Action::Released,
        Action::Canceled,
    ];

    for (i, action) in actions.iter().enumerate() {
        let message = Message::new_order(
            Some(uuid::Uuid::new_v4()),
            Some((12345 + i) as u64),
            Some(1),
            action.clone(),
            None,
        );
        let timestamp = (1700000000 + i * 60) as u64;
        msgs.push((message, timestamp, sender_keys.public_key()));
    }

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_dispute_payload() {
    let sender_keys = Keys::generate();
    let dispute_id = uuid::Uuid::new_v4();
    let dispute_payload = Payload::Dispute(dispute_id, None);
    let message = Message::new_dispute(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::DisputeInitiatedByYou,
        Some(dispute_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_orders_payload() {
    let sender_keys = Keys::generate();
    let order = SmallOrder {
        id: Some(uuid::Uuid::new_v4()),
        kind: Some(mostro_core::order::Kind::Buy),
        status: Some(Status::Active),
        amount: 10000,
        fiat_code: "USD".to_string(),
        fiat_amount: 100,
        payment_method: "cash".to_string(),
        premium: 0,
        created_at: Some(1700000000),
        expires_at: Some(1700086400),
        buyer_invoice: None,
        buyer_trade_pubkey: None,
        seller_trade_pubkey: None,
        min_amount: None,
        max_amount: None,
    };
    let orders_payload = Payload::Orders(vec![order]);
    let message = Message::new_order(
        None,
        Some(12345),
        Some(1),
        Action::Orders,
        Some(orders_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_distinguishes_mostro() {
    let mostro_keys = Keys::generate();
    let sender_keys = Keys::generate();

    let msg1 = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::NewOrder,
        None,
    );
    let msg2 = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12346),
        Some(1),
        Action::PayInvoice,
        None,
    );

    let msgs = vec![
        (msg1, 1700000000u64, mostro_keys.public_key()),
        (msg2, 1700000060u64, sender_keys.public_key()),
    ];

    let res = print_direct_messages(&msgs, Some(mostro_keys.public_key())).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_restore_session_payload() {
    let sender_keys = Keys::generate();
    let order_info = RestoredOrdersInfo {
        order_id: uuid::Uuid::new_v4(),
        trade_index: 1,
        status: "active".to_string(),
    };
    let dispute_info = RestoredDisputesInfo {
        dispute_id: uuid::Uuid::new_v4(),
        order_id: uuid::Uuid::new_v4(),
        trade_index: 1,
        status: "initiated".to_string(),
    };
    let restore_payload = Payload::RestoreData(RestoreSessionInfo {
        restore_orders: vec![order_info],
        restore_disputes: vec![dispute_info],
    });
    let message = Message::new_order(
        None,
        Some(12345),
        Some(1),
        Action::RestoreSession,
        Some(restore_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn parse_dm_with_time_filter() {
    let keys = Keys::generate();
    let events = Events::new(&Filter::new());
    let since = 1700000000i64;
    let out = parse_dm_events(events, &keys, Some(&since)).await;
    assert!(out.is_empty());
}

#[tokio::test]
async fn print_dms_with_long_details_truncation() {
    let sender_keys = Keys::generate();
    let long_text = "A".repeat(200);
    let text_payload = Payload::TextMessage(long_text);
    let message = Message::new_dm(None, None, Action::SendDm, Some(text_payload));
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_rating_action() {
    let sender_keys = Keys::generate();
    let rating_payload = Payload::RatingUser(5);
    let message = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::RateReceived,
        Some(rating_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_add_invoice_action() {
    let sender_keys = Keys::generate();
    let order = SmallOrder {
        id: Some(uuid::Uuid::new_v4()),
        kind: Some(mostro_core::order::Kind::Sell),
        status: Some(Status::WaitingBuyerInvoice),
        amount: 50000,
        fiat_code: "EUR".to_string(),
        fiat_amount: 500,
        payment_method: "revolut".to_string(),
        premium: 2,
        buyer_trade_pubkey: None,
        seller_trade_pubkey: None,
        buyer_invoice: None,
        created_at: Some(1700000000),
        expires_at: Some(1700086400),
        min_amount: None,
        max_amount: None,
    };
    let order_payload = Payload::Order(order);
    let message = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::AddInvoice,
        Some(order_payload),
    );
    let timestamp = 1700000000u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn print_dms_with_invalid_timestamp() {
    let sender_keys = Keys::generate();
    let message = Message::new_order(
        Some(uuid::Uuid::new_v4()),
        Some(12345),
        Some(1),
        Action::NewOrder,
        None,
    );
    let timestamp = 0u64;
    let msgs = vec![(message, timestamp, sender_keys.public_key())];

    let res = print_direct_messages(&msgs, None).await;
    assert!(res.is_ok());
}
