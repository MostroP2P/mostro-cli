//! `admsetmaintenance` / `admmaintenancestatus`: drive `mostrod`'s
//! maintenance (drain) mode over the admin gRPC. These talk to the daemon
//! directly, not over Nostr, so they need neither relays nor `ADMIN_NSEC`.

use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::rpc::{AdminRpcClient, GetMaintenanceStatusResponse, RpcConfig, RPC_URL_ENV};
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
        "Unpaid dev fees",
        &c.unpaid_dev_fees.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🪢 ",
        "Open bonds",
        &c.open_bonds.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🪢 ",
        "Pending bond payouts",
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
    if s.drained {
        out.push_str("✅ Nothing is bound to the Lightning node: safe to stop mostrod and switch [lightning].\n");
    } else {
        out.push_str(
            "⏳ Escrow is still bound to the Lightning node; keep it online and poll again.\n",
        );
    }
    out
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

    #[test]
    fn render_tolerates_missing_optionals() {
        let out = render_status(&GetMaintenanceStatusResponse::default());
        assert!(out.contains("OFF"));
        assert!(!out.contains("Reason"));
        assert!(!out.contains("Since"));
        assert!(!out.contains("Stored LN"));
    }
}
