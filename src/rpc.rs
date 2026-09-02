//! Minimal client for `mostrod`'s admin gRPC (`proto/admin.proto` in the
//! daemon repo, service `mostro.admin.v1.AdminService`).
//!
//! Only the maintenance-mode calls are wired: everything else the admin
//! needs already travels over Nostr. The messages are hand-written `prost`
//! structs mirroring the proto (field numbers matter, names do not), so
//! building the CLI needs neither `protoc` nor a `build.rs`.
//!
//! Endpoint and credentials come from the environment:
//! - `MOSTRO_RPC_URL` (default `http://127.0.0.1:50051`)
//! - `MOSTRO_RPC_TOKEN` (optional; sent as `authorization: Bearer <token>`,
//!   required when the daemon sets `[rpc].auth_token`)

use anyhow::{anyhow, Context as _, Result};
use std::time::Duration;
use tonic::client::Grpc;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

pub const RPC_URL_ENV: &str = "MOSTRO_RPC_URL";
pub const RPC_TOKEN_ENV: &str = "MOSTRO_RPC_TOKEN";
pub const DEFAULT_RPC_URL: &str = "http://127.0.0.1:50051";

const SERVICE: &str = "mostro.admin.v1.AdminService";
/// A black-holed `MOSTRO_RPC_URL` must fail fast, not hang until the OS gives up.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A server that accepts the connection but never answers.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, PartialEq, prost::Message)]
pub struct CancelOrderRequest {
    #[prost(string, tag = "1")]
    pub order_id: String,
    #[prost(string, optional, tag = "2")]
    pub request_id: Option<String>,
    /// Refuse anything that is not still `pending` / `waiting-taker-bond`
    /// instead of falling through to the dispute-resolution cancel
    /// (MostroP2P/mostro#944).
    #[prost(bool, optional, tag = "3")]
    pub pretrade_only: Option<bool>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct CancelOrderResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(string, optional, tag = "2")]
    pub error_message: Option<String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SetMaintenanceModeRequest {
    #[prost(bool, tag = "1")]
    pub enabled: bool,
    #[prost(string, optional, tag = "2")]
    pub reason: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub request_id: Option<String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SetMaintenanceModeResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(string, optional, tag = "2")]
    pub error_message: Option<String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct GetMaintenanceStatusRequest {
    #[prost(string, optional, tag = "1")]
    pub request_id: Option<String>,
}

/// What is still bound to the daemon's connected Lightning node.
#[derive(Clone, PartialEq, prost::Message)]
pub struct DrainCounters {
    #[prost(uint32, tag = "1")]
    pub escrowed_orders: u32,
    #[prost(uint32, tag = "2")]
    pub inflight_payouts: u32,
    #[prost(uint32, tag = "3")]
    pub inflight_dev_fees: u32,
    #[prost(uint32, tag = "4")]
    pub open_bonds: u32,
    #[prost(uint32, tag = "5")]
    pub pending_bond_payouts: u32,
    #[prost(uint32, tag = "6")]
    pub pending_orders: u32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct GetMaintenanceStatusResponse {
    #[prost(bool, tag = "1")]
    pub enabled: bool,
    #[prost(string, optional, tag = "2")]
    pub reason: Option<String>,
    #[prost(int64, optional, tag = "3")]
    pub since: Option<i64>,
    #[prost(message, optional, tag = "4")]
    pub counters: Option<DrainCounters>,
    #[prost(bool, tag = "5")]
    pub drained: bool,
    #[prost(string, tag = "6")]
    pub ln_node_pubkey: String,
    #[prost(string, optional, tag = "7")]
    pub stored_ln_node_pubkey: Option<String>,
}

/// Connection settings, resolved from the environment by [`RpcConfig::from_env`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcConfig {
    pub url: String,
    pub token: Option<String>,
}

impl RpcConfig {
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var(RPC_URL_ENV).ok(),
            std::env::var(RPC_TOKEN_ENV).ok(),
        )
    }

    /// Pure resolution rule: blank values count as unset, the URL falls back
    /// to [`DEFAULT_RPC_URL`], the token is trimmed.
    pub fn from_values(url: Option<String>, token: Option<String>) -> Self {
        let clean = |v: Option<String>| v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        Self {
            url: clean(url).unwrap_or_else(|| DEFAULT_RPC_URL.to_owned()),
            token: clean(token),
        }
    }
}

/// Refuse to send a bearer token in cleartext anywhere but to the local
/// machine. `mostrod` itself only serves plaintext gRPC and only accepts
/// `SetMaintenanceMode` from loopback peers, so the supported shapes are a
/// loopback URL (directly or through an SSH tunnel) or `https://` via a
/// TLS-terminating proxy in front of the daemon.
pub fn check_token_transport(url: &str, has_token: bool) -> Result<()> {
    if !has_token {
        return Ok(());
    }
    let uri: tonic::codegen::http::Uri = url
        .parse()
        .map_err(|e| anyhow!("invalid {RPC_URL_ENV}: {url}: {e}"))?;
    let scheme = uri.scheme_str().unwrap_or("http");
    if scheme == "https" {
        return Ok(());
    }
    let host = uri.host().unwrap_or("");
    let is_loopback = host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if is_loopback {
        Ok(())
    } else {
        Err(anyhow!(
            "refusing to send {RPC_TOKEN_ENV} in cleartext to {url}: use a loopback URL \
             (e.g. an SSH tunnel to the daemon host) or an https:// endpoint"
        ))
    }
}

/// `authorization` header value for a configured token, or `None`.
pub fn bearer_header(token: Option<&str>) -> Result<Option<MetadataValue<tonic::metadata::Ascii>>> {
    token
        .map(|t| {
            format!("Bearer {t}")
                .parse()
                .map_err(|_| anyhow!("{RPC_TOKEN_ENV} contains characters not allowed in a header"))
        })
        .transpose()
}

pub struct AdminRpcClient {
    inner: Grpc<Channel>,
    auth: Option<MetadataValue<tonic::metadata::Ascii>>,
    url: String,
}

impl AdminRpcClient {
    pub async fn connect(config: &RpcConfig) -> Result<Self> {
        Self::connect_with_timeouts(config, CONNECT_TIMEOUT, REQUEST_TIMEOUT).await
    }

    pub async fn connect_with_timeouts(
        config: &RpcConfig,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self> {
        check_token_transport(&config.url, config.token.is_some())?;
        let endpoint = Endpoint::from_shared(config.url.clone())
            .with_context(|| format!("invalid {RPC_URL_ENV}: {}", config.url))?
            .connect_timeout(connect_timeout)
            .timeout(request_timeout);
        let endpoint = if config.url.starts_with("https://") {
            endpoint
                .tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots())
                .context("cannot configure TLS for the admin RPC")?
        } else {
            endpoint
        };
        let channel = endpoint.connect().await.with_context(|| {
            format!(
                "cannot reach mostrod admin RPC at {} (connect timeout {}s)",
                config.url,
                connect_timeout.as_secs()
            )
        })?;
        Ok(Self {
            inner: Grpc::new(channel),
            auth: bearer_header(config.token.as_deref())?,
            url: config.url.clone(),
        })
    }

    async fn unary<Req, Resp>(&mut self, method: &'static str, body: Req) -> Result<Resp>
    where
        Req: prost::Message + 'static,
        Resp: prost::Message + Default + 'static,
    {
        self.inner
            .ready()
            .await
            .with_context(|| format!("admin RPC at {} is not ready", self.url))?;
        let mut request = Request::new(body);
        if let Some(auth) = &self.auth {
            request.metadata_mut().insert("authorization", auth.clone());
        }
        let path = PathAndQuery::try_from(format!("/{SERVICE}/{method}"))
            .map_err(|e| anyhow!("bad gRPC path for {method}: {e}"))?;
        let codec = tonic_prost::ProstCodec::<Req, Resp>::default();
        let response = self
            .inner
            .unary(request, path, codec)
            .await
            .map_err(|status| describe_status(method, &status))?;
        Ok(response.into_inner())
    }

    pub async fn set_maintenance_mode(
        &mut self,
        enabled: bool,
        reason: Option<String>,
    ) -> Result<SetMaintenanceModeResponse> {
        self.unary(
            "SetMaintenanceMode",
            SetMaintenanceModeRequest {
                enabled,
                reason,
                request_id: None,
            },
        )
        .await
    }

    /// `CancelOrder` from the daemon key, restricted to a still-pending
    /// (`pending` / `waiting-taker-bond`) order: bonds released, maker
    /// notified. `pretrade_only` makes the daemon refuse anything else —
    /// in particular a dispute the daemon has taken, which the same RPC
    /// would otherwise resolve as the solver (cancel escrow, refund seller).
    pub async fn cancel_pending_order(&mut self, order_id: &str) -> Result<CancelOrderResponse> {
        self.unary(
            "CancelOrder",
            CancelOrderRequest {
                order_id: order_id.to_owned(),
                request_id: None,
                pretrade_only: Some(true),
            },
        )
        .await
    }

    pub async fn get_maintenance_status(&mut self) -> Result<GetMaintenanceStatusResponse> {
        self.unary(
            "GetMaintenanceStatus",
            GetMaintenanceStatusRequest { request_id: None },
        )
        .await
    }
}

/// Turn a gRPC status into an operator-readable error, with a hint for the
/// two refusals the daemon documents.
pub fn describe_status(method: &str, status: &tonic::Status) -> anyhow::Error {
    let hint = match status.code() {
        tonic::Code::PermissionDenied => {
            " (hint: SetMaintenanceMode is loopback-only; if the daemon sets [rpc].auth_token, \
             export MOSTRO_RPC_TOKEN)"
        }
        tonic::Code::Unimplemented => {
            " (hint: the daemon predates maintenance mode; upgrade mostrod)"
        }
        _ => "",
    };
    anyhow!(
        "{method} failed: {} {}{hint}",
        status.code(),
        status.message()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn config_defaults_and_trims() {
        assert_eq!(
            RpcConfig::from_values(None, None),
            RpcConfig {
                url: DEFAULT_RPC_URL.into(),
                token: None
            }
        );
        assert_eq!(
            RpcConfig::from_values(Some("  ".into()), Some("".into())),
            RpcConfig {
                url: DEFAULT_RPC_URL.into(),
                token: None
            },
            "blank values are unset"
        );
        assert_eq!(
            RpcConfig::from_values(
                Some(" http://10.0.0.5:50051 ".into()),
                Some(" s3cret\n".into())
            ),
            RpcConfig {
                url: "http://10.0.0.5:50051".into(),
                token: Some("s3cret".into())
            }
        );
    }

    #[test]
    fn bearer_header_formats_and_validates() {
        assert!(bearer_header(None).unwrap().is_none());
        let h = bearer_header(Some("s3cret")).unwrap().unwrap();
        assert_eq!(h.to_str().unwrap(), "Bearer s3cret");
        assert!(bearer_header(Some("bad\nvalue")).is_err());
    }

    /// `CancelOrderRequest` field numbers match `proto/admin.proto`.
    #[test]
    fn cancel_request_encodes_with_proto_field_numbers() {
        let bytes = CancelOrderRequest {
            order_id: "ab".into(),
            request_id: None,
            pretrade_only: Some(true),
        }
        .encode_to_vec();
        // field 1 len-delimited = 0x0a 0x02 'a' 'b'; no field 2; field 3
        // varint = 0x18 0x01
        assert_eq!(bytes, vec![0x0a, 0x02, b'a', b'b', 0x18, 0x01]);
        let resp = CancelOrderResponse::decode(&[0x08, 0x00, 0x12, 0x01, b'e'][..]).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.error_message.as_deref(), Some("e"));
    }

    /// Field numbers are the wire contract with `proto/admin.proto`; pin the
    /// encoding so a renumbering here cannot silently talk past the daemon.
    #[test]
    fn set_request_encodes_with_proto_field_numbers() {
        let bytes = SetMaintenanceModeRequest {
            enabled: true,
            reason: Some("x".into()),
            request_id: None,
        }
        .encode_to_vec();
        // field 1 varint = 0x08 0x01; field 2 len-delimited = 0x12 0x01 'x'
        assert_eq!(bytes, vec![0x08, 0x01, 0x12, 0x01, b'x']);
    }

    #[test]
    fn status_response_round_trips_with_nested_counters() {
        let resp = GetMaintenanceStatusResponse {
            enabled: true,
            reason: Some("ln migration".into()),
            since: Some(1_700_000_000),
            counters: Some(DrainCounters {
                escrowed_orders: 2,
                inflight_payouts: 1,
                inflight_dev_fees: 0,
                open_bonds: 3,
                pending_bond_payouts: 0,
                pending_orders: 7,
            }),
            drained: false,
            ln_node_pubkey: "02aa".into(),
            stored_ln_node_pubkey: Some("02bb".into()),
        };
        let decoded =
            GetMaintenanceStatusResponse::decode(resp.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded, resp);
        // Tag 4 (counters) must be a length-delimited nested message.
        assert!(resp.encode_to_vec().contains(&0x22));
    }

    #[test]
    fn token_transport_rule() {
        // No token: anything goes.
        assert!(check_token_transport("http://10.0.0.5:50051", false).is_ok());
        // Token over cleartext to loopback (direct or tunnelled) is fine.
        for url in [
            "http://127.0.0.1:50051",
            "http://localhost:50051",
            "http://[::1]:50051",
            "http://127.5.5.5:50051",
        ] {
            assert!(check_token_transport(url, true).is_ok(), "{url}");
        }
        // Token over TLS to anywhere is fine.
        assert!(check_token_transport("https://mostro.example:443", true).is_ok());
        // Token over cleartext to a remote host is refused.
        for url in ["http://10.0.0.5:50051", "http://mostro.example:50051"] {
            let err = check_token_transport(url, true).unwrap_err();
            assert!(err.to_string().contains("cleartext"), "{url}: {err}");
        }
    }

    /// A black-holed address must fail within the connect timeout, not hang.
    #[tokio::test]
    async fn connect_honours_the_connect_timeout() {
        let config = RpcConfig::from_values(Some("http://10.255.255.1:50051".into()), None);
        let started = std::time::Instant::now();
        let result = AdminRpcClient::connect_with_timeouts(
            &config,
            Duration::from_millis(300),
            Duration::from_secs(1),
        )
        .await;
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
        match result {
            Err(e) => assert!(e.to_string().contains("connect timeout"), "{e}"),
            Ok(_) => panic!("a black-holed address must not connect"),
        }
    }

    #[test]
    fn describe_status_adds_hints() {
        let denied = describe_status(
            "SetMaintenanceMode",
            &tonic::Status::permission_denied("nope"),
        );
        assert!(denied.to_string().contains("MOSTRO_RPC_TOKEN"));
        let old = describe_status("GetMaintenanceStatus", &tonic::Status::unimplemented(""));
        assert!(old.to_string().contains("upgrade mostrod"));
        let other = describe_status("X", &tonic::Status::internal("boom"));
        assert!(other.to_string().contains("boom") && !other.to_string().contains("hint"));
    }
}
