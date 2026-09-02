//! `admsetmaintenance` / `admmaintenancestatus` / `admcancelpending`: drive
//! `mostrod`'s maintenance (drain) mode over the admin gRPC. These talk to
//! the daemon directly, not over Nostr, so they need neither relays nor
//! `ADMIN_NSEC`.

use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::rpc::{
    ensure_pretrade_only_enforced, AdminRpcClient, GetMaintenanceStatusResponse, RpcConfig,
    RPC_URL_ENV,
};
use anyhow::{anyhow, Result};

pub async fn execute_set_maintenance(enabled: bool, reason: Option<String>) -> Result<()> {
    let config = RpcConfig::from_env();
    println!("👑 Admin Set Maintenance Mode");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row("🔌 ", RPC_URL_ENV, &config.url));
    table.add_row(create_emoji_field_row(
        "🛠️ ",
        "Enabled",
        if enabled { "true" } else { "false" },
    ));
    if let Some(r) = &reason {
        table.add_row(create_emoji_field_row("📝 ", "Reason", r));
    }
    println!("{table}");

    let mut client = AdminRpcClient::connect(&config).await?;
    let resp = client.set_maintenance_mode(enabled, reason).await?;
    if !resp.success {
        return Err(anyhow!(
            "daemon refused the change: {}",
            resp.error_message.unwrap_or_else(|| "unknown error".into())
        ));
    }
    if enabled {
        println!("✅ Maintenance mode is ON: new orders and takes are rejected; open trades keep working.");
        println!("💡 Poll `mostro-cli admmaintenancestatus` until it reports drained = true.");
    } else {
        println!("✅ Maintenance mode is OFF: the order book is open again.");
    }
    Ok(())
}

/// Operator cancel of a pre-trade order through the daemon's `CancelOrder`
/// gRPC with `pretrade_only` set. Unlike `admcancel` (Nostr, `ADMIN_NSEC`,
/// disputes) this reaches the daemon-key path that accepts `pending` /
/// `waiting-taker-bond` orders, releasing the maker's bond at once — the
/// way to shorten the maintenance drain instead of waiting for
/// `max_expiration_days`. The daemon refuses any other status (a dispute
/// included), so a mistyped id can never resolve a trade. Because an older
/// daemon would silently ignore the flag, the daemon version is checked
/// first (`ensure_pretrade_only_enforced`).
pub async fn execute_cancel_pending(order_id: uuid::Uuid) -> Result<()> {
    let config = RpcConfig::from_env();
    println!("👑 Admin Cancel Pending Order");
    println!("═══════════════════════════════════════");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row("🔌 ", RPC_URL_ENV, &config.url));
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order id",
        &order_id.to_string(),
    ));
    println!("{table}");

    let mut client = AdminRpcClient::connect(&config).await?;
    // Capability gate before anything that could touch an order: an older
    // daemon ignores `pretrade_only` and would resolve a dispute instead.
    let daemon = client.get_version().await?.version;
    ensure_pretrade_only_enforced(&daemon)?;
    let resp = client.cancel_pending_order(&order_id.to_string()).await?;
    if !resp.success {
        return Err(anyhow!(
            "daemon refused the cancel: {}",
            resp.error_message.unwrap_or_else(|| "unknown error".into())
        ));
    }
    println!("{}", cancel_pending_ok(order_id));
    Ok(())
}

/// Pure success line, testable without a daemon.
pub fn cancel_pending_ok(order_id: uuid::Uuid) -> String {
    format!(
        "✅ Order {order_id} cancelled by the operator: maker notified, bonds released.\n\
         💡 Run `mostro-cli admmaintenancestatus` to watch open_bonds drop."
    )
}

pub async fn execute_maintenance_status() -> Result<()> {
    let config = RpcConfig::from_env();
    let mut client = AdminRpcClient::connect(&config).await?;
    let status = client.get_maintenance_status().await?;
    print!("{}", render_status(&status));
    Ok(())
}

/// Pure rendering of the status, so it is testable without a daemon.
pub fn render_status(s: &GetMaintenanceStatusResponse) -> String {
    let mut out = String::new();
    out.push_str("👑 Mostro Maintenance Status\n");
    out.push_str("═══════════════════════════════════════\n");
    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "🛠️ ",
        "Maintenance mode",
        if s.enabled { "ON" } else { "OFF" },
    ));
    if let Some(r) = &s.reason {
        table.add_row(create_emoji_field_row("📝 ", "Reason", r));
    }
    if let Some(since) = s.since {
        let when = chrono::DateTime::from_timestamp(since, 0)
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| since.to_string());
        table.add_row(create_emoji_field_row("⏱️ ", "Since", &when));
    }
    let c = s.counters.clone().unwrap_or_default();
    table.add_row(create_emoji_field_row(
        "🔒 ",
        "Escrowed orders",
        &c.escrowed_orders.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "✈️ ",
        "In-flight payouts",
        &c.inflight_payouts.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "💸 ",
        "In-flight dev fees",
        &c.inflight_dev_fees.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🪢 ",
        "Open bonds",
        &c.open_bonds.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🪢 ",
        "In-flight bond payouts",
        &c.pending_bond_payouts.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Pending orders (no escrow)",
        &c.pending_orders.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        if s.drained { "✅ " } else { "⏳ " },
        "Drained",
        if s.drained { "true" } else { "false" },
    ));
    table.add_row(create_emoji_field_row(
        "⚡ ",
        "LN node pubkey",
        &s.ln_node_pubkey,
    ));
    if let Some(stored) = &s.stored_ln_node_pubkey {
        table.add_row(create_emoji_field_row(
            "💾 ",
            "Stored LN node pubkey",
            stored,
        ));
    }
    out.push_str(&format!("{table}\n"));
    out.push_str(verdict(s));
    out.push('\n');
    out
}

/// The operator-facing verdict. "Safe to switch" needs BOTH conditions: with
/// the book open a drained daemon can take on new node-bound escrow the
/// moment after the operator reads this line.
fn verdict(s: &GetMaintenanceStatusResponse) -> &'static str {
    match (s.enabled, s.drained) {
        (true, true) => {
            "✅ Maintenance mode is ON and nothing is bound to the Lightning node: safe to stop mostrod and switch [lightning]."
        }
        (true, false) => {
            "⏳ Escrow is still bound to the Lightning node; keep it online and poll again."
        }
        (false, true) => {
            "⚠️ Nothing is bound right now, but the book is OPEN: new escrow can arrive at any moment. Run `admsetmaintenance --enabled true` first, then poll again."
        }
        (false, false) => {
            "⚠️ Escrow is bound to the Lightning node and the book is OPEN. Run `admsetmaintenance --enabled true` to stop new escrow, then poll until drained."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::DrainCounters;

    fn status(drained: bool) -> GetMaintenanceStatusResponse {
        GetMaintenanceStatusResponse {
            enabled: true,
            reason: Some("ln migration".into()),
            since: Some(1_700_000_000),
            counters: Some(DrainCounters {
                escrowed_orders: if drained { 0 } else { 2 },
                ..Default::default()
            }),
            drained,
            ln_node_pubkey: "02aa".into(),
            stored_ln_node_pubkey: Some("02bb".into()),
        }
    }

    #[test]
    fn render_reports_drained_verdict_and_fields() {
        let out = render_status(&status(false));
        assert!(out.contains("ON"));
        assert!(out.contains("ln migration"));
        assert!(out.contains("2023-11-14T22:13:20+00:00"));
        assert!(out.contains("02aa") && out.contains("02bb"));
        assert!(out.contains("keep it online"));

        let out = render_status(&status(true));
        assert!(out.contains("safe to stop mostrod"));
    }

    /// "Safe to switch" must never be printed while the book is open.
    #[test]
    fn verdict_requires_maintenance_on_and_drained() {
        let mut s = status(true);
        assert!(verdict(&s).contains("safe to stop"));
        s.enabled = false;
        let v = verdict(&s);
        assert!(!v.contains("safe to stop") && v.contains("OPEN") && v.contains("--enabled true"));
        s.drained = false;
        let v = verdict(&s);
        assert!(!v.contains("safe to stop") && v.contains("--enabled true"));
        s.enabled = true;
        assert!(verdict(&s).contains("keep it online"));
        let out = render_status(&GetMaintenanceStatusResponse::default());
        assert!(
            !out.contains("safe to stop"),
            "default (OFF, drained) is not safe"
        );
    }

    #[test]
    fn cancel_pending_ok_names_the_order_and_next_step() {
        let id = uuid::Uuid::new_v4();
        let out = cancel_pending_ok(id);
        assert!(out.contains(&id.to_string()));
        assert!(out.contains("admmaintenancestatus"));
    }

    #[test]
    fn render_tolerates_missing_optionals() {
        let out = render_status(&GetMaintenanceStatusResponse::default());
        assert!(out.contains("OFF"));
        assert!(!out.contains("Reason"));
        assert!(!out.contains("Since"));
        assert!(!out.contains("Stored LN"));
    }
}
