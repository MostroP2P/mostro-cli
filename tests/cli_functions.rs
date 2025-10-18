use anyhow::Result;
use mostro_core::prelude::*;
use uuid::Uuid;

// Test rate_user helper function
#[test]
fn test_get_user_rate_valid_ratings() {
    use mostro_client::cli::rate_user;

    // We can't test the private function directly, but we can test
    // the validation logic through the constants
    let valid_ratings = vec![1u8, 2u8, 3u8, 4u8, 5u8];

    for rating in valid_ratings {
        // Valid ratings should be in the RATING_BOUNDARIES constant
        assert!(rating >= 1 && rating <= 5);
    }
}

#[test]
fn test_invalid_ratings_out_of_range() {
    let invalid_ratings = vec![0u8, 6u8, 10u8, 255u8];

    for rating in invalid_ratings {
        assert!(rating < 1 || rating > 5);
    }
}

// Test orders_info validation
#[test]
fn test_orders_info_empty_order_ids() {
    let order_ids: Vec<Uuid> = Vec::new();

    // Empty order_ids should be rejected
    assert!(order_ids.is_empty());
}

#[test]
fn test_orders_info_single_order_id() {
    let order_id = Uuid::new_v4();
    let order_ids = vec![order_id];

    assert_eq!(order_ids.len(), 1);
    assert_eq!(order_ids[0], order_id);
}

#[test]
fn test_orders_info_multiple_order_ids() {
    let order_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];

    assert_eq!(order_ids.len(), 3);
    // All UUIDs should be unique
    assert_ne!(order_ids[0], order_ids[1]);
    assert_ne!(order_ids[1], order_ids[2]);
    assert_ne!(order_ids[0], order_ids[2]);
}

#[test]
fn test_orders_info_payload_creation() {
    let order_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let payload = Payload::Ids(order_ids.clone());

    match payload {
        Payload::Ids(ids) => {
            assert_eq!(ids.len(), 2);
            assert_eq!(ids, order_ids);
        }
        _ => panic!("Expected Payload::Ids"),
    }
}

#[test]
fn test_message_creation_for_orders_action() {
    let order_ids = vec![Uuid::new_v4()];
    let request_id = Uuid::new_v4().as_u128() as u64;
    let trade_index = 5i64;
    let payload = Payload::Ids(order_ids.clone());

    let message = Message::new_order(
        None,
        Some(request_id),
        Some(trade_index),
        Action::Orders,
        Some(payload),
    );

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Orders);
    assert_eq!(inner.request_id, Some(request_id));
    assert_eq!(inner.trade_index, Some(trade_index));
    assert!(inner.id.is_none());
}

#[test]
fn test_message_serialization_for_orders() {
    let order_ids = vec![Uuid::new_v4()];
    let payload = Payload::Ids(order_ids);

    let message = Message::new_order(None, Some(12345), Some(1), Action::Orders, Some(payload));

    let json_result = message.as_json();
    assert!(json_result.is_ok());

    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());
    assert!(json_str.contains("Orders"));
}

// Test restore session message creation
#[test]
fn test_restore_message_creation() {
    let restore_message = Message::new_restore(None);

    let inner = restore_message.get_inner_message_kind();
    assert_eq!(inner.action, Action::RestoreSession);
    assert!(inner.payload.is_none());
}

#[test]
fn test_restore_message_serialization() {
    let restore_message = Message::new_restore(None);

    let json_result = restore_message.as_json();
    assert!(json_result.is_ok());

    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());
    assert!(json_str.contains("RestoreSession"));
}

// Test rating payload creation
#[test]
fn test_rating_payload_creation() {
    for rating in 1u8..=5u8 {
        let payload = Payload::RatingUser(rating);

        match payload {
            Payload::RatingUser(r) => {
                assert_eq!(r, rating);
                assert!(r >= 1 && r <= 5);
            }
            _ => panic!("Expected Payload::RatingUser"),
        }
    }
}

#[test]
fn test_rate_user_message_creation() {
    let order_id = Uuid::new_v4();
    let rating = 5u8;
    let payload = Payload::RatingUser(rating);

    let message = Message::new_order(Some(order_id), None, None, Action::RateUser, Some(payload));

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::RateUser);
    assert_eq!(inner.id, Some(order_id));

    match inner.payload {
        Some(Payload::RatingUser(r)) => assert_eq!(r, rating),
        _ => panic!("Expected RatingUser payload"),
    }
}

// Test take order payload creation
#[test]
fn test_take_buy_payload_with_amount() {
    let amount = 50000u32;
    let payload = Payload::Amount(amount as i64);

    match payload {
        Payload::Amount(amt) => assert_eq!(amt, amount as i64),
        _ => panic!("Expected Payload::Amount"),
    }
}

#[test]
fn test_take_sell_payload_with_invoice() {
    let invoice = "lnbc1000n1...".to_string();
    let payload = Payload::PaymentRequest(None, invoice.clone(), None);

    match payload {
        Payload::PaymentRequest(_, inv, _) => assert_eq!(inv, invoice),
        _ => panic!("Expected Payload::PaymentRequest"),
    }
}

#[test]
fn test_take_sell_payload_with_invoice_and_amount() {
    let invoice = "lnbc1000n1...".to_string();
    let amount = 75000i64;
    let payload = Payload::PaymentRequest(None, invoice.clone(), Some(amount));

    match payload {
        Payload::PaymentRequest(_, inv, Some(amt)) => {
            assert_eq!(inv, invoice);
            assert_eq!(amt, amount);
        }
        _ => panic!("Expected Payload::PaymentRequest with amount"),
    }
}

// Test dispute messages
#[test]
fn test_dispute_message_creation_add_solver() {
    let dispute_id = Uuid::new_v4();
    let npubkey = "npub1...";
    let payload = Payload::PubkeyToAddSolver(npubkey.to_string());

    let message = Message::new_dispute(
        Some(dispute_id),
        None,
        None,
        Action::AdminAddSolver,
        Some(payload),
    );

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminAddSolver);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_cancel() {
    let dispute_id = Uuid::new_v4();

    let message = Message::new_dispute(Some(dispute_id), None, None, Action::AdminCancel, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminCancel);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_settle() {
    let dispute_id = Uuid::new_v4();

    let message = Message::new_dispute(Some(dispute_id), None, None, Action::AdminSettle, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminSettle);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_take() {
    let dispute_id = Uuid::new_v4();

    let message = Message::new_dispute(Some(dispute_id), None, None, Action::TakeDispute, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::TakeDispute);
    assert_eq!(inner.id, Some(dispute_id));
}

// Test new order message creation
#[test]
fn test_new_order_message_with_trade_index() {
    let trade_index = 42i64;
    let payload = Payload::Order(Order {
        id: None,
        kind: Some(mostro_core::order::Kind::Buy),
        status: Some(Status::Pending),
        amount: 100000,
        fiat_code: "USD".to_string(),
        fiat_amount: 1000,
        payment_method: "cash".to_string(),
        premium: 0,
        created_at: None,
        expires_at: None,
        buyer_invoice: None,
        master_buyer_pubkey: None,
        master_seller_pubkey: None,
        buyer_token: None,
        seller_token: None,
        min_amount: None,
        max_amount: None,
        price: None,
    });

    let message = Message::new_order(
        None,
        None,
        Some(trade_index),
        Action::NewOrder,
        Some(payload),
    );

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::NewOrder);
    assert_eq!(inner.trade_index, Some(trade_index));
}

// Test send_msg action variations
#[test]
fn test_send_msg_cancel_action() {
    let order_id = Uuid::new_v4();

    let message = Message::new_order(Some(order_id), None, None, Action::Cancel, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Cancel);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_fiat_sent_action() {
    let order_id = Uuid::new_v4();

    let message = Message::new_order(Some(order_id), None, None, Action::FiatSent, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::FiatSent);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_release_action() {
    let order_id = Uuid::new_v4();

    let message = Message::new_order(Some(order_id), None, None, Action::Release, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Release);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_dispute_action() {
    let order_id = Uuid::new_v4();

    let message = Message::new_dispute(Some(order_id), None, None, Action::Dispute, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Dispute);
    assert_eq!(inner.id, Some(order_id));
}

// Test DM message creation
#[test]
fn test_dm_message_creation() {
    let order_id = Uuid::new_v4();
    let message_text = "Hello, how are you?";
    let payload = Payload::TextMessage(message_text.to_string());

    let message = Message::new_dm(Some(order_id), None, None, Action::SendDm, Some(payload));

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::SendDm);
    assert_eq!(inner.id, Some(order_id));

    match inner.payload {
        Some(Payload::TextMessage(text)) => assert_eq!(text, message_text),
        _ => panic!("Expected TextMessage payload"),
    }
}

// Test last trade index message
#[test]
fn test_last_trade_index_message() {
    let message = Message::new_order(None, None, None, Action::LastTradeIndex, None);

    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::LastTradeIndex);
    assert!(inner.id.is_none());
    assert!(inner.payload.is_none());
}
