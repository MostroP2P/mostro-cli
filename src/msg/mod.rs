//! Public Nostr reads: node configuration, read limits and the collector.
//!
//! Only configuration, limits, `fetch_msg` and its results are exported. The
//! client of a public read lives in the private `Session`; the compiler, not a
//! convention, keeps it from callers:
//!
//! ```compile_fail,E0603
//! use mostro_client::msg::Session;
//! ```
//!
//! The order decoder and reducer helpers are private as well; callers use the
//! re-exported functions:
//!
//! ```compile_fail,E0603
//! use mostro_client::msg::orders::reduce_orders;
//! ```
//!
//! So are the doors of the request and reply exchange: publishing, waiting for the
//! reply and reading the node's information happen only inside `exchange`.
//!
//! ```compile_fail,E0603
//! use mostro_client::msg::send_msg::publish;
//! ```
//!
//! ```compile_fail,E0603
//! use mostro_client::msg::reply::arm;
//! ```
//!
//! ```compile_fail,E0603
//! use mostro_client::msg::node_info::probe;
//! ```
//!
//! The exported surface itself is stable:
//!
//! ```
//! use mostro_client::msg::{
//!     exchange, fetch_msg, ExchangeFailure, ExchangeLimits, Exchanged, Expect, FetchFailure,
//!     Fetched, Outbound, PublicReadConfig, ReadLimits,
//! };
//! ```

use std::str::FromStr;
use std::time::Duration;

use nostr_sdk::prelude::{Client, Keys, PublicKey, RelayUrl, SignerAuthenticator};

use crate::outcome::ErrorCode;
use crate::util::net::install_rustls_crypto_provider;

pub mod exchange;
mod node_info;
mod orders;
pub mod receive_msg;
mod reply;
mod send_msg;

pub use exchange::{exchange, ExchangeFailure, ExchangeLimits, Exchanged, Expect, Outbound};
pub(crate) use orders::{reduce_orders, seven_day_filter};
pub use receive_msg::{fetch_msg, FetchFailure, Fetched};

pub const MOSTRO_PUBKEY_VAR: &str = "MOSTRO_PUBKEY";
pub const RELAYS_VAR: &str = "RELAYS";

/// Public node configuration: the Mostro key and deduplicated relay URLs.
/// It holds no identity, database or credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicReadConfig {
    mostro_pubkey: PublicKey,
    relays: Vec<RelayUrl>,
}

impl PublicReadConfig {
    /// Resolve from CLI values, falling back to `env` only for values the CLI
    /// did not supply. An empty or invalid CLI value never falls back. `env`
    /// is asked for `MOSTRO_PUBKEY` and `RELAYS` only.
    pub fn resolve(
        mostro_pubkey: Option<&str>,
        relays: Option<&str>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ErrorCode> {
        let mostro_pubkey = present(
            mostro_pubkey
                .map(str::to_owned)
                .or_else(|| env(MOSTRO_PUBKEY_VAR)),
        )?;
        let relays = present(relays.map(str::to_owned).or_else(|| env(RELAYS_VAR)))?;
        let mostro_pubkey =
            PublicKey::from_str(&mostro_pubkey).map_err(|_| ErrorCode::ConfigurationInvalid)?;
        Self::new(mostro_pubkey, &relays)
    }

    /// `relays` is comma-separated like `RELAYS`: entries are trimmed, empty
    /// entries dropped, URLs validated and duplicates removed.
    pub fn new(mostro_pubkey: PublicKey, relays: &str) -> Result<Self, ErrorCode> {
        let mut parsed: Vec<RelayUrl> = Vec::new();
        for entry in relay_entries(relays) {
            let url = RelayUrl::parse(entry).map_err(|_| ErrorCode::ConfigurationInvalid)?;
            // `RelayUrl` equality compares normalized URLs.
            if !parsed.contains(&url) {
                parsed.push(url);
            }
        }
        if parsed.is_empty() {
            return Err(ErrorCode::ConfigurationMissing);
        }
        Ok(Self {
            mostro_pubkey,
            relays: parsed,
        })
    }

    pub fn mostro_pubkey(&self) -> PublicKey {
        self.mostro_pubkey
    }

    pub fn relays(&self) -> &[RelayUrl] {
        &self.relays
    }
}

/// Bounds of one public read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimits {
    /// Budget for connecting every configured relay.
    pub connect: Duration,
    /// Budget from the REQ until every relay's EOSE.
    pub read: Duration,
    /// Budget for unsubscribing and disconnecting.
    pub cleanup: Duration,
    /// Unique matching events retained before the read fails.
    pub max_unique_events: usize,
    /// Per-relay notification buffer. The SDK skips lagged notifications
    /// silently, so a relay producing this many before its EOSE fails the read.
    pub notification_capacity: usize,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(15),
            cleanup: Duration::from_secs(1),
            max_unique_events: 10_000,
            notification_capacity: 2_048,
        }
    }
}

fn present(value: Option<String>) -> Result<String, ErrorCode> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(ErrorCode::ConfigurationMissing)
}

/// Entries of a comma-separated relay list, trimmed, without empty ones.
pub(crate) fn relay_entries(relays: &str) -> impl Iterator<Item = &str> {
    relays
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
}

/// A client that answers NIP-42 challenges with fresh in-memory keys only.
pub(crate) fn ephemeral_client() -> Client {
    install_rustls_crypto_provider();
    Client::builder()
        .authenticator(SignerAuthenticator::new(Keys::generate()))
        .build()
}

/// The connected client of one public read; it is private to this module tree.
struct Session {
    client: Client,
}

impl Session {
    /// Connect every configured relay within `limits.connect`, without
    /// automatic reconnection. Any relay left unconnected fails the session.
    async fn connect(config: &PublicReadConfig, limits: &ReadLimits) -> Result<Self, ErrorCode> {
        let session = Self {
            client: ephemeral_client(),
        };
        for url in &config.relays {
            session
                .client
                .add_relay(url)
                .reconnect(false)
                .connect_timeout(limits.connect)
                .notification_channel_size(limits.notification_capacity)
                .await
                .map_err(|_| ErrorCode::InternalError)?;
        }
        let connected = session.client.try_connect().timeout(limits.connect).await;
        if connected.failed.is_empty() && connected.success.len() == config.relays.len() {
            Ok(session)
        } else {
            Err(ErrorCode::RelayConnectionFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::net::Ipv4Addr;
    use std::time::Instant;

    use nostr_sdk::prelude::ToBech32;
    use tokio::net::TcpListener;

    use super::*;

    const NODE_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const RELAY: &str = "wss://relay.example";

    /// Resolve with `env` answering from `vars`, returning the names asked for.
    fn resolve(
        cli_pubkey: Option<&str>,
        cli_relays: Option<&str>,
        vars: &[(&str, &str)],
    ) -> (Result<PublicReadConfig, ErrorCode>, Vec<String>) {
        let asked = RefCell::new(Vec::new());
        let env = |name: &str| {
            asked.borrow_mut().push(name.to_string());
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        };
        let config = PublicReadConfig::resolve(cli_pubkey, cli_relays, env);
        (config, asked.into_inner())
    }

    fn relays(config: &PublicReadConfig) -> Vec<&str> {
        config.relays().iter().map(RelayUrl::as_str).collect()
    }

    #[test]
    fn cli_values_win_and_environment_fills_the_rest() {
        let env = [
            (MOSTRO_PUBKEY_VAR, "not-a-key"),
            (RELAYS_VAR, "wss://env.example"),
        ];

        let (config, asked) = resolve(Some(NODE_HEX), None, &env);
        let config = config.unwrap();
        assert_eq!(config.mostro_pubkey().to_hex(), NODE_HEX);
        assert_eq!(relays(&config), ["wss://env.example"]);
        assert_eq!(asked, [RELAYS_VAR]);

        let (config, asked) = resolve(Some(NODE_HEX), Some(RELAY), &env);
        assert_eq!(relays(&config.unwrap()), [RELAY]);
        assert!(asked.is_empty(), "{asked:?}");
    }

    #[test]
    fn empty_or_invalid_cli_values_do_not_fall_back() {
        let env = [(MOSTRO_PUBKEY_VAR, NODE_HEX), (RELAYS_VAR, RELAY)];
        for (pubkey, relays, expected) in [
            (Some(""), None, ErrorCode::ConfigurationMissing),
            (Some("  "), None, ErrorCode::ConfigurationMissing),
            (Some("not-a-key"), None, ErrorCode::ConfigurationInvalid),
            (None, Some(""), ErrorCode::ConfigurationMissing),
            (None, Some(" , ,"), ErrorCode::ConfigurationMissing),
            (
                None,
                Some("https://relay.example"),
                ErrorCode::ConfigurationInvalid,
            ),
            (
                None,
                Some("wss://ok.example,not a url"),
                ErrorCode::ConfigurationInvalid,
            ),
        ] {
            let (config, _) = resolve(pubkey, relays, &env);
            assert_eq!(config, Err(expected), "{pubkey:?} {relays:?}");
        }
    }

    #[test]
    fn missing_values_are_configuration_missing() {
        assert_eq!(
            resolve(None, None, &[]).0,
            Err(ErrorCode::ConfigurationMissing)
        );
        assert_eq!(
            resolve(None, None, &[(RELAYS_VAR, RELAY)]).0,
            Err(ErrorCode::ConfigurationMissing)
        );
        assert_eq!(
            resolve(None, None, &[(MOSTRO_PUBKEY_VAR, NODE_HEX)]).0,
            Err(ErrorCode::ConfigurationMissing)
        );
    }

    #[test]
    fn npub_is_accepted_and_kept_as_the_same_key() {
        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let (config, _) = resolve(Some(&npub), Some(RELAY), &[]);
        assert_eq!(config.unwrap().mostro_pubkey(), keys.public_key());
    }

    #[test]
    fn relays_are_trimmed_and_deduplicated_after_normalization() {
        let config = PublicReadConfig::new(
            Keys::generate().public_key(),
            " wss://Relay.Example:443 ,,wss://relay.example/, ws://other.example ,",
        )
        .unwrap();
        assert_eq!(config.relays().len(), 2);
        assert_eq!(
            config.relays()[1],
            RelayUrl::parse("ws://other.example").unwrap()
        );
    }

    #[test]
    fn only_public_variables_are_read() {
        let env = [
            ("ADMIN_NSEC", "nsec-sentinel"),
            ("MOSTRO_RPC_TOKEN", "token-sentinel"),
            (MOSTRO_PUBKEY_VAR, NODE_HEX),
            (RELAYS_VAR, RELAY),
        ];
        let (config, asked) = resolve(None, None, &env);
        assert!(config.is_ok());
        assert_eq!(asked, [MOSTRO_PUBKEY_VAR, RELAYS_VAR]);
        assert!(!format!("{:?}", config.unwrap()).contains("sentinel"));
    }

    #[tokio::test]
    async fn an_unanswered_relay_fails_the_session_within_the_budget() {
        // Bound but never accepted: the WebSocket handshake can only time out.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let config = PublicReadConfig::new(Keys::generate().public_key(), &url).unwrap();
        let limits = ReadLimits {
            connect: Duration::from_millis(300),
            ..ReadLimits::default()
        };

        let started = Instant::now();
        let session = Session::connect(&config, &limits).await;

        assert!(matches!(session, Err(ErrorCode::RelayConnectionFailed)));
        assert!(started.elapsed() < limits.connect + Duration::from_secs(2));
    }
}
