use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use crate::cli::Context;
use crate::db::{is_row_not_found, Order};
use crate::lightning::is_valid_invoice;
use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::util::{
    fetch_events_list, print_dm_events, save_order, send_dm, wait_for_dm, Event, ListKind,
};

/// Create payload based on action type and parameters
fn create_take_order_payload(
    action: Action,
    invoice: &Option<String>,
    amount: Option<u32>,
) -> Result<Option<Payload>> {
    match action {
        Action::TakeBuy => Ok(amount.map(|amt: u32| Payload::Amount(amt as i64))),
        Action::TakeSell => Ok(Some(match invoice {
            Some(inv) => {
                let initial_payload = match LightningAddress::from_str(inv) {
                    Ok(_) => Payload::PaymentRequest(None, inv.to_string(), None),
                    Err(_) => match is_valid_invoice(inv) {
                        Ok(i) => Payload::PaymentRequest(None, i.to_string(), None),
                        Err(e) => {
                            println!("{}", e);
                            Payload::PaymentRequest(None, inv.to_string(), None)
                        }
                    },
                };

                match amount {
                    Some(amt) => match initial_payload {
                        Payload::PaymentRequest(a, b, _) => {
                            Payload::PaymentRequest(a, b, Some(amt as i64))
                        }
                        payload => payload,
                    },
                    None => initial_payload,
                }
            }
            None => amount
                .map(|amt| Payload::Amount(amt.into()))
                .unwrap_or(Payload::Amount(0)),
        })),
        _ => Err(anyhow::anyhow!("Invalid action for take order")),
    }
}

/// Unified function to handle both take buy and take sell orders
#[allow(clippy::too_many_arguments)]
pub async fn execute_take_order(
    order_id: &Uuid,
    action: Action,
    invoice: &Option<String>,
    amount: Option<u32>,
    ctx: &Context,
) -> Result<()> {
    let action_name = match &action {
        Action::TakeBuy => "take buy",
        Action::TakeSell => "take sell",
        _ => return Err(anyhow::anyhow!("Invalid action for take order")),
    };

    println!("🛒 Take Order");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row("📈 ", "Action", action_name));
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order ID",
        &order_id.to_string(),
    ));
    if let Some(inv) = invoice {
        table.add_row(create_emoji_field_row("⚡ ", "Invoice", inv));
    }
    if let Some(amt) = amount {
        table.add_row(create_emoji_field_row(
            "💵 ",
            "Fiat Amount",
            &amt.to_string(),
        ));
    }
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Mostro PubKey",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Taking order from Mostro...\n");

    // Create payload based on action type
    let payload = create_take_order_payload(action.clone(), invoice, amount)?;

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create message
    let take_order_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(ctx.trade_index),
        action.clone(),
        payload,
    );

    // Send dm to receiver pubkey
    println!("📤 Sending Message");
    println!("─────────────────────────────────────");
    println!("🔢 Trade Index: {}", ctx.trade_index);
    println!("🔑 Trade Keys: {}", ctx.trade_keys.public_key().to_hex());
    println!("💡 Sending DM to Mostro...");
    println!();

    let message_json = take_order_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the DM and wait for Mostro's kind-14 reply in the main thread
    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Wait for the DM to be sent from mostro
    let recv_event = wait_for_dm(ctx, None, sent_message).await?;

    // Parse the incoming DM
    print_dm_events(recv_event, request_id, ctx, None).await?;

    // The node answers a takesell that carried the invoice with
    // waiting-seller-to-pay and no order (#204), so no reply above saved it.
    if action == Action::TakeSell && invoice.is_some() {
        save_taken_sell_order(order_id, amount, request_id, ctx).await?;
    }

    Ok(())
}

/// Saves a sell order this client has just taken with its invoice, unless a
/// row for it exists already (a bond-invoice reply saves it first). Like the
/// other takes, this records the take's trade index as the last one used.
async fn save_taken_sell_order(
    order_id: &Uuid,
    fiat_amount: Option<u32>,
    request_id: u64,
    ctx: &Context,
) -> Result<()> {
    match Order::get_by_id(&ctx.pool, &order_id.to_string()).await {
        Ok(_) => return Ok(()),
        Err(e) if is_row_not_found(&e) => {}
        Err(e) => return Err(e),
    }
    let listed = book_order(order_id, ctx).await;
    if listed.is_none() {
        println!("⚠️  Order {order_id} is not in the book: saving its id and trade keys only");
    }
    let order = taken_sell_order(*order_id, listed, fiat_amount);
    save_order(
        order,
        &ctx.trade_keys,
        request_id,
        ctx.trade_index,
        &ctx.pool,
    )
    .await
}

/// The latest revision of `order_id` in the node's book (Kind 38383).
async fn book_order(order_id: &Uuid, ctx: &Context) -> Option<SmallOrder> {
    match fetch_events_list(ListKind::Orders, None, None, None, ctx, None).await {
        Ok(events) => events.into_iter().find_map(|event| match event {
            Event::SmallOrder(order) if order.id == Some(*order_id) => Some(order),
            _ => None,
        }),
        Err(e) => {
            log::warn!("could not read order {order_id} from the book: {e}");
            None
        }
    }
}

/// The local row for a sell order taken with an invoice: the book's copy of
/// it, now waiting for the seller to pay. Without one, its id and side:
/// along with the trade keys, that is what the next commands need.
fn taken_sell_order(
    order_id: Uuid,
    listed: Option<SmallOrder>,
    fiat_amount: Option<u32>,
) -> SmallOrder {
    let mut order = listed.unwrap_or_else(|| {
        SmallOrder::new(
            Some(order_id),
            Some(mostro_core::order::Kind::Sell),
            None,
            0,
            String::new(),
            None,
            None,
            0,
            String::new(),
            0,
            None,
            None,
            None,
            None,
            None,
        )
    });
    order.id = Some(order_id);
    order.status = Some(Status::WaitingPayment);
    if let Some(amount) = fiat_amount {
        // A range order, taken for part of its range.
        order.fiat_amount = amount.into();
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;
    use sqlx::SqlitePool;

    const ORDER: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

    fn order_id() -> Uuid {
        Uuid::parse_str(ORDER).unwrap()
    }

    fn listed(min_max: Option<(i64, i64)>) -> SmallOrder {
        SmallOrder::new(
            Some(order_id()),
            Some(mostro_core::order::Kind::Sell),
            Some(Status::Pending),
            12_000,
            "USD".into(),
            min_max.map(|(min, _)| min),
            min_max.map(|(_, max)| max),
            if min_max.is_some() { 0 } else { 12 },
            "bank transfer".into(),
            0,
            None,
            None,
            None,
            None,
            None,
        )
    }

    async fn pool_with_user(last_trade_index: i64) -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            r#"
            CREATE TABLE orders (
                id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL,
                amount INTEGER NOT NULL, min_amount INTEGER, max_amount INTEGER,
                fiat_code TEXT NOT NULL, fiat_amount INTEGER NOT NULL,
                payment_method TEXT NOT NULL, premium INTEGER NOT NULL,
                trade_keys TEXT, counterparty_pubkey TEXT, is_mine BOOLEAN,
                buyer_invoice TEXT, request_id INTEGER, created_at INTEGER,
                expires_at INTEGER, solver_pubkey TEXT
            );
            CREATE TABLE users (
                i0_pubkey char(64) PRIMARY KEY, mnemonic TEXT,
                last_trade_index INTEGER, created_at INTEGER
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO users VALUES (?, 'test', ?, 0)")
            .bind("ab".repeat(32))
            .bind(last_trade_index)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    /// A context whose client has no relays: the book cannot be read.
    fn offline_ctx(pool: SqlitePool, trade: &Keys, trade_index: i64) -> Context {
        Context {
            client: Client::builder()
                .authenticator(SignerAuthenticator::new(trade.clone()))
                .build(),
            identity_keys: trade.clone(),
            trade_keys: trade.clone(),
            trade_index,
            pool,
            context_keys: None,
            mostro_pubkey: Keys::generate().public_key(),
        }
    }

    #[test]
    fn the_listed_order_is_kept_and_now_waits_for_the_seller() {
        let order = taken_sell_order(order_id(), Some(listed(None)), None);
        assert_eq!(order.status, Some(Status::WaitingPayment));
        assert_eq!(order.amount, 12_000);
        assert_eq!(order.fiat_code, "USD");
        assert_eq!(order.fiat_amount, 12);
        assert_eq!(order.payment_method, "bank transfer");
    }

    #[test]
    fn a_range_order_records_the_amount_taken() {
        let order = taken_sell_order(order_id(), Some(listed(Some((100, 500)))), Some(150));
        assert_eq!(order.fiat_amount, 150);
        assert_eq!((order.min_amount, order.max_amount), (Some(100), Some(500)));
    }

    #[test]
    fn without_the_book_the_row_still_has_its_id_and_side() {
        let order = taken_sell_order(order_id(), None, None);
        assert_eq!(order.id, Some(order_id()));
        assert_eq!(order.kind, Some(mostro_core::order::Kind::Sell));
        assert_eq!(order.status, Some(Status::WaitingPayment));
    }

    #[tokio::test]
    async fn a_taken_order_is_saved_with_the_take_keys_and_advances_the_index() {
        let trade = Keys::generate();
        let ctx = offline_ctx(pool_with_user(3).await, &trade, 4);

        save_taken_sell_order(&order_id(), None, 77, &ctx)
            .await
            .unwrap();

        let row = Order::get_by_id(&ctx.pool, ORDER).await.unwrap();
        assert_eq!(row.trade_keys, Some(trade.secret_key().to_secret_hex()));
        assert_eq!(row.status.as_deref(), Some("waiting-payment"));
        assert_eq!(row.kind.as_deref(), Some("sell"));
        assert_eq!(row.request_id, Some(77));
        let user = crate::db::User::get(&ctx.pool).await.unwrap();
        assert_eq!(user.last_trade_index, Some(4));
    }

    #[tokio::test]
    async fn an_order_already_saved_is_left_alone() {
        let first = Keys::generate();
        let ctx = offline_ctx(pool_with_user(3).await, &first, 4);
        save_taken_sell_order(&order_id(), None, 77, &ctx)
            .await
            .unwrap();

        let second = Keys::generate();
        let again = offline_ctx(ctx.pool.clone(), &second, 5);
        save_taken_sell_order(&order_id(), None, 78, &again)
            .await
            .unwrap();

        let row = Order::get_by_id(&ctx.pool, ORDER).await.unwrap();
        assert_eq!(row.trade_keys, Some(first.secret_key().to_secret_hex()));
        assert_eq!(row.request_id, Some(77));
    }

    #[tokio::test]
    async fn a_trade_index_that_cannot_advance_is_an_error() {
        // The order row is written, but no user row can take the index: the
        // next take would reuse it, so this take must not report success.
        let pool = pool_with_user(3).await;
        sqlx::query("DROP TABLE users")
            .execute(&pool)
            .await
            .unwrap();
        let ctx = offline_ctx(pool, &Keys::generate(), 4);

        let result = save_taken_sell_order(&order_id(), None, 77, &ctx).await;

        assert!(result.is_err(), "{result:?}");
    }

    #[tokio::test]
    async fn an_order_that_cannot_be_written_is_an_error() {
        // The row is missing, as after any such take, and the insert fails.
        let pool = pool_with_user(3).await;
        sqlx::query(
            "CREATE TRIGGER refuse_orders BEFORE INSERT ON orders \
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();
        let ctx = offline_ctx(pool, &Keys::generate(), 4);

        let result = save_taken_sell_order(&order_id(), None, 77, &ctx).await;

        assert!(result.is_err(), "{result:?}");
    }
}
