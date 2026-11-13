//! Dispatch of `admin order-info`: a question signed with the key of the
//! node's admin or of one of its solvers. Everything that can fail locally
//! fails before anything is published, in order of cost: the node
//! configuration, then the admin key. No local identity or store is opened.
//!
//! The admin key comes from the environment only (`ADMIN_NSEC`): the config
//! file refuses it. It is read here and nowhere else on this path, and it is
//! never printed: a bad value is reported by a code, without the value.

use std::env;

use nostr_sdk::prelude::Keys;

use super::commands::AdminOrderInfoArgs;
use super::invocation::OutputMode;
use super::write::{exchange_limits, finish, node_lookup, request_options};
use super::Cli;
use crate::commands::admin::order_info::{admin_order_info, AdminOrderInfoOutcome};
use crate::msg::PublicReadConfig;
use crate::outcome::admin_order_info::COMMAND;
use crate::outcome::{DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Publication, Warnings};
use crate::parse_human::render_admin_order_info;
use crate::settings::env_text;

/// The environment variable holding the admin's or a solver's private key.
const ADMIN_KEY: &str = "ADMIN_NSEC";
const UNWRITABLE: &str = "error: the order information could not be written";

/// Run `admin order-info` and return its exit status.
pub(super) async fn order_info(cli: &Cli, args: &AdminOrderInfoArgs, output: OutputMode) -> i32 {
    let outcome = order_info_outcome(cli, args).await;
    // The outcome is never uncertain: nothing is written, so there is no advice.
    finish(
        cli,
        output,
        &outcome,
        render_admin_order_info,
        (UNWRITABLE, ""),
    )
}

async fn order_info_outcome(cli: &Cli, args: &AdminOrderInfoArgs) -> AdminOrderInfoOutcome {
    let lookup = match node_lookup() {
        Ok(lookup) => lookup,
        Err(code) => return unsent(code),
    };
    let config = match PublicReadConfig::resolve(
        cli.mostro_pubkey.as_deref(),
        cli.relays.as_deref(),
        &lookup,
    ) {
        Ok(config) => config,
        Err(code) => return unsent(code),
    };
    let options = match request_options(cli.pow.as_deref(), cli.transport.as_deref(), &lookup) {
        Ok(options) => options,
        Err(code) => return unsent(code),
    };
    let admin = match admin_keys(env_text(env::var_os(ADMIN_KEY)).as_deref()) {
        Ok(keys) => keys,
        Err(code) => return unsent(code),
    };
    admin_order_info(args.order, &config, &admin, options, &exchange_limits(cli)).await
}

/// The admin key from the text of `ADMIN_NSEC`: a private key as `nsec1...`
/// or 64 hex characters. An unset or blank variable is `admin_key_missing`;
/// anything else that is not a private key is `admin_key_invalid`. The value
/// never leaves this function in an error.
fn admin_keys(value: Option<&str>) -> Result<Keys, ErrorCode> {
    let text = value.map(str::trim).filter(|text| !text.is_empty());
    let text = text.ok_or(ErrorCode::AdminKeyMissing)?;
    Keys::parse(text).map_err(|_| ErrorCode::AdminKeyInvalid)
}

/// A failure before anything was published.
fn unsent(code: ErrorCode) -> AdminOrderInfoOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::ToBech32;

    #[test]
    fn an_unset_or_blank_variable_is_admin_key_missing() {
        for value in [None, Some(""), Some("   "), Some("\n")] {
            assert_eq!(
                admin_keys(value).unwrap_err(),
                ErrorCode::AdminKeyMissing,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_value_that_is_not_a_private_key_is_admin_key_invalid() {
        let public = Keys::generate().public_key();
        for value in [
            "not-a-key",
            "nsec1qqqq",
            &public.to_bech32().unwrap(),
            &"z".repeat(64),
        ] {
            assert_eq!(
                admin_keys(Some(value)).unwrap_err(),
                ErrorCode::AdminKeyInvalid,
                "{value}"
            );
        }
    }

    #[test]
    fn an_nsec_and_its_hex_are_the_same_key() {
        let keys = Keys::generate();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hex = keys.secret_key().to_secret_hex();

        for value in [nsec.as_str(), hex.as_str(), &format!("  {nsec}\n")] {
            assert_eq!(
                admin_keys(Some(value)).unwrap().public_key(),
                keys.public_key()
            );
        }
    }

    /// The outcome of a bad key names the variable and never its value.
    #[test]
    fn the_failure_does_not_carry_the_value() {
        const SENTINEL: &str = "secret-sentinel-7f3a";
        let code = admin_keys(Some(SENTINEL)).unwrap_err();
        let outcome = serde_json::to_string(&unsent(code)).unwrap();
        assert!(outcome.contains("admin_key_invalid"), "{outcome}");
        assert!(!outcome.contains(SENTINEL), "{outcome}");
        assert!(!format!("{code:?}").contains(SENTINEL));
    }
}
