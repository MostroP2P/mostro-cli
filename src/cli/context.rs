use anyhow::{anyhow, Result};
use log::{info, warn};
use nostr_sdk::prelude::{Client, Keys, PublicKey};
use pretty_env_logger::init as init_logger;
use sqlx::SqlitePool;
use std::env::{set_var, var};
use std::str::FromStr;

use super::args::Cli;
use super::commands::Commands;
use crate::db::{connect, User};
use crate::util::connect_nostr;
use crate::util::events::fetch_protocol_version_with;
use crate::util::messaging::parse_transport_env;

#[derive(Debug)]
pub struct Context {
    pub client: Client,
    pub identity_keys: Keys,
    pub trade_keys: Keys,
    pub trade_index: i64,
    pub pool: SqlitePool,
    pub context_keys: Option<Keys>,
    pub mostro_pubkey: PublicKey,
}

fn get_env_var(cli: &Cli) {
    // Init logger
    if cli.verbose {
        set_var("RUST_LOG", "info");
        init_logger();
    }

    if let Some(ref mostro_pubkey) = cli.mostro_pubkey {
        set_var("MOSTRO_PUBKEY", mostro_pubkey.clone());
    }

    if let Some(ref relays) = cli.relays {
        set_var("RELAYS", relays.clone());
    }

    if let Some(ref pow) = cli.pow {
        set_var("POW", pow.clone());
    }

    if cli.secret {
        set_var("SECRET", "true");
    }

    if let Some(ref transport) = cli.transport {
        set_var("TRANSPORT", transport.clone());
    }
}

/// Shared footnote for missing `MOSTRO_PUBKEY` / `RELAYS` errors.
///
/// The CLI never reads a config file; `~/.config/mostro` is a user-managed
/// convenience (see README Suggested setup), not something the binary creates.
const ENV_FILE_HINT: &str = "\
mostro-cli does not read any config file, and does not create ~/.config/mostro.\n\
To avoid re-exporting every time, create that directory yourself, put the\n\
exports in a file (e.g. ~/.config/mostro/env.sh, chmod 600) and `source` it\n\
before running the CLI.\n\
See https://github.com/MostroP2P/mostro-cli#suggested-setup";

/// Resolve the Mostro instance pubkey from `--mostro-pubkey` / `-m`, then
/// `MOSTRO_PUBKEY`. Fails with setup guidance if neither is set.
fn resolve_mostro_pubkey(cli: &Cli) -> Result<String> {
    cli.mostro_pubkey
        .clone()
        .or_else(|| var("MOSTRO_PUBKEY").ok())
        .ok_or_else(|| {
            anyhow!(
                "MOSTRO_PUBKEY not set.\n\
                 Provide it using one of the following methods:\n\
                 1) --mostro-pubkey <npub> (or -m)\n\
                 2) export MOSTRO_PUBKEY=<npub>\n\
                 \n\
                 {ENV_FILE_HINT}"
            )
        })
}

/// Ensure `RELAYS` is configured (and non-empty) before we create any local
/// state. `connect_nostr` reads this variable; validating it here means a
/// command run without relays fails before the mnemonic database is created.
/// `get_env_var` has already mirrored `--relays` into the env var by this point.
fn resolve_relays() -> Result<String> {
    match var("RELAYS") {
        Ok(relays) if !relays.trim().is_empty() => Ok(relays),
        _ => Err(anyhow!(
            "RELAYS not set.\n\
             Provide it using one of the following methods:\n\
             1) --relays <relay[,relay...]> (or -r)\n\
             2) export RELAYS=<relay[,relay...]>\n\
             \n\
             {ENV_FILE_HINT}"
        )),
    }
}

pub(super) async fn init_context(cli: &Cli) -> Result<Context> {
    // Get environment variables
    get_env_var(cli);

    // Validate all required configuration *before* touching the local database.
    // `connect()` creates `~/.mcli/mcli.db` and generates the mnemonic on first
    // run, so a mistaken command with missing config must fail here, without
    // ever writing that sensitive material to disk. See issue #179.
    let mostro_pubkey = PublicKey::from_str(&resolve_mostro_pubkey(cli)?)?;
    resolve_relays()?;

    // Load private key of admin - only required for admin commands.
    // For regular user commands, this will be None. Validated up front so an
    // admin command with a missing/invalid ADMIN_NSEC also fails before the
    // database is created.
    let context_keys = if is_admin_command(&cli.command) {
        Some(
            var("ADMIN_NSEC")
                .map_err(|e| anyhow!("ADMIN_NSEC not set (required for admin commands): {}", e))?
                .parse::<Keys>()
                .map_err(|e| anyhow!("Failed to parse ADMIN_NSEC: {}", e))?,
        )
    } else {
        None
    };

    // Initialize database pool (creates the DB and mnemonic on first run)
    let pool = connect().await?;

    // Get identity keys
    let identity_keys = User::get_identity_keys(&pool)
        .await
        .map_err(|e| anyhow!("Failed to get identity keys: {}", e))?;

    // Get trade keys
    let (trade_keys, trade_index) = User::get_next_trade_keys(&pool)
        .await
        .map_err(|e| anyhow!("Failed to get trade keys: {}", e))?;

    // Connect to Nostr relays
    let client = connect_nostr().await?;

    // Resolve the wire transport once, at startup. An explicit
    // `--transport`/`TRANSPORT` wins; otherwise auto-detect from the node's
    // advertised `protocol_version` so the operator need not match it by hand
    // (docs/TRANSPORT_V2_SPEC.md Phase 3).
    resolve_transport(&client, mostro_pubkey).await?;

    Ok(Context {
        client,
        identity_keys,
        trade_keys,
        trade_index,
        pool,
        context_keys,
        mostro_pubkey,
    })
}

/// Resolve the wire transport into the `TRANSPORT` env var the messaging layer
/// reads (`parse_transport_env`). An explicit `--transport` / `TRANSPORT` is
/// authoritative and skips the network probe; otherwise the node's advertised
/// `protocol_version` tag (kind-38385 info event) selects it. Protocol v1
/// (gift wrap) is no longer supported. A node that publishes nothing defaults
/// to nip44.
async fn resolve_transport(client: &Client, mostro_pubkey: PublicKey) -> Result<()> {
    // Only an explicit, non-empty value is authoritative — mirror
    // `parse_transport_env`, which treats empty/whitespace as unset.
    if let Ok(explicit) = var("TRANSPORT") {
        if !explicit.trim().is_empty() {
            parse_transport_env()?;
            info!("Transport: {explicit} (explicit)");
            return Ok(());
        }
    }
    match fetch_protocol_version_with(client.clone(), mostro_pubkey).await {
        Some(2) => {
            set_var("TRANSPORT", "nip44");
            info!("Transport: nip44 (auto-detected protocol v2)");
        }
        Some(1) => {
            return Err(anyhow!(
                "This Mostro instance speaks protocol v1 (gift wrap), which this CLI no longer supports. \
                 Use a v2 node (protocol_version=2 / nip44)."
            ));
        }
        Some(other) => {
            set_var("TRANSPORT", "nip44");
            warn!("Node advertised unknown protocol_version={other}; defaulting to nip44");
        }
        None => {
            set_var("TRANSPORT", "nip44");
            info!(
                "Could not detect node transport (no kind-38385 protocol_version tag); \
                 defaulting to nip44"
            );
        }
    }
    Ok(())
}

fn is_admin_command(command: &Option<Commands>) -> bool {
    matches!(
        command,
        Some(Commands::AdmCancel { .. })
            | Some(Commands::AdmSettle { .. })
            | Some(Commands::AdmAddSolver { .. })
            | Some(Commands::AdmTakeDispute { .. })
            | Some(Commands::AdmSendDm { .. })
            | Some(Commands::GetAdminDm { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serial_test::serial;
    use std::env::remove_var;

    fn with_env_cleared<R>(key: &str, f: impl FnOnce() -> R) -> R {
        let previous = var(key).ok();
        remove_var(key);
        let result = f();
        match previous {
            Some(value) => set_var(key, value),
            None => remove_var(key),
        }
        result
    }

    #[test]
    #[serial]
    fn missing_mostro_pubkey_message_guides_setup() {
        with_env_cleared("MOSTRO_PUBKEY", || {
            let cli = Cli::try_parse_from(["mostro-cli", "listdisputes"]).unwrap();
            let err = resolve_mostro_pubkey(&cli).unwrap_err().to_string();
            assert!(err.contains("--mostro-pubkey <npub>"), "{err}");
            assert!(err.contains("(or -m)"), "{err}");
            assert!(err.contains("export MOSTRO_PUBKEY=<npub>"), "{err}");
            assert!(err.contains("does not read any config file"), "{err}");
            assert!(err.contains("does not create ~/.config/mostro"), "{err}");
            assert!(err.contains("chmod 600"), "{err}");
            assert!(
                err.contains("https://github.com/MostroP2P/mostro-cli#suggested-setup"),
                "{err}"
            );
        });
    }

    #[test]
    #[serial]
    fn missing_relays_message_guides_setup() {
        with_env_cleared("RELAYS", || {
            let err = resolve_relays().unwrap_err().to_string();
            assert!(err.contains("--relays <relay[,relay...]>"), "{err}");
            assert!(err.contains("(or -r)"), "{err}");
            assert!(err.contains("export RELAYS=<relay[,relay...]>"), "{err}");
            assert!(err.contains("does not read any config file"), "{err}");
            assert!(err.contains("does not create ~/.config/mostro"), "{err}");
            assert!(
                err.contains("https://github.com/MostroP2P/mostro-cli#suggested-setup"),
                "{err}"
            );
        });
    }

    #[test]
    #[serial]
    fn mostro_pubkey_flag_wins_when_env_is_unset() {
        with_env_cleared("MOSTRO_PUBKEY", || {
            let cli = Cli::try_parse_from(["mostro-cli", "-m", "npub1testpubkey", "listdisputes"])
                .unwrap();
            assert_eq!(resolve_mostro_pubkey(&cli).unwrap(), "npub1testpubkey");
        });
    }
}
