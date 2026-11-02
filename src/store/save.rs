//! The one statement that writes an order row, and the transaction around it:
//! the row and the trade index are written together or not at all.

use mostro_core::order::SmallOrder;
use sqlx::SqlitePool;

use super::{Role, StoreError, Trade};

/// What to do with a row that already exists.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Existing {
    /// Overwrite it with the saved order.
    Replace,
    /// Leave it exactly as it is.
    Keep,
}

/// Save `order`; `true` when a row was written (always for [`Existing::Replace`]).
pub(super) async fn save_in(
    pool: &SqlitePool,
    order: &SmallOrder,
    trade: &Trade,
    request_id: u64,
    (role, existing): (Role, Existing),
) -> Result<bool, StoreError> {
    let (Some(id), Some(kind), Some(status)) =
        (order.id, order.kind.as_ref(), order.status.as_ref())
    else {
        return Err(StoreError::Unwritable);
    };
    let unwritable = |_| StoreError::Unwritable;
    let mut transaction = pool.begin().await.map_err(unwritable)?;
    // Two fixed statements that differ only in what a conflict does.
    macro_rules! insert_order {
        ($conflict:literal) => {
            concat!(
                "INSERT INTO orders (id, kind, status, amount, min_amount, max_amount, \
                 fiat_code, fiat_amount, payment_method, premium, trade_keys, is_mine, \
                 request_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, \
                 ?9, ?10, ?11, ?15, ?12, ?13, ?14) ON CONFLICT(id) ",
                $conflict
            )
        };
    }
    let sql = match existing {
        Existing::Replace => insert_order!(
            "DO UPDATE SET kind = ?2, status = ?3, amount = ?4, min_amount = ?5, \
             max_amount = ?6, fiat_code = ?7, fiat_amount = ?8, payment_method = ?9, \
             premium = ?10, trade_keys = ?11, is_mine = ?15, request_id = ?12, expires_at = ?14"
        ),
        Existing::Keep => insert_order!("DO NOTHING"),
    };
    let written = sqlx::query(sql)
        .bind(id.to_string())
        .bind(kind.to_string())
        .bind(status.to_string())
        .bind(order.amount)
        .bind(order.min_amount)
        .bind(order.max_amount)
        .bind(&order.fiat_code)
        .bind(order.fiat_amount)
        .bind(&order.payment_method)
        .bind(order.premium)
        .bind(trade.keys.secret_key().to_secret_hex())
        .bind(request_id as i64)
        .bind(chrono::Utc::now().timestamp())
        .bind(order.expires_at)
        .bind(role == Role::Maker)
        .execute(&mut *transaction)
        .await
        .map_err(unwritable)?
        .rows_affected()
        > 0;
    sqlx::query("UPDATE users SET last_trade_index = ?1 WHERE COALESCE(last_trade_index, 0) < ?1")
        .bind(trade.index)
        .execute(&mut *transaction)
        .await
        .map_err(unwritable)?;
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *transaction)
        .await
        .map_err(unwritable)?;
    if users == 0 {
        // Dropping the transaction rolls the order row back.
        return Err(StoreError::Missing);
    }
    transaction.commit().await.map_err(unwritable)?;
    Ok(written)
}
