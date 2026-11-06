//! `identity.init` and `identity.show` data: the public identity key of the
//! local store and, for `show`, the last trade index recorded with it.
//!
//! Only public values are serialized. The mnemonic, every private key, the
//! store's path and database errors never reach these types.

use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;

use super::{schema, ErrorCode};

/// Canonical name of `identity init`.
pub const INIT_COMMAND: &str = "identity.init";
/// Canonical name of `identity show`.
pub const SHOW_COMMAND: &str = "identity.show";

/// Codes that fail `identity.init`.
pub const INIT_ERRORS: [ErrorCode; 4] = [
    ErrorCode::InvalidArguments,
    ErrorCode::InternalError,
    ErrorCode::IdentityExists,
    ErrorCode::StoreUnwritable,
];

/// Codes that fail `identity.show`.
pub const SHOW_ERRORS: [ErrorCode; 4] = [
    ErrorCode::InvalidArguments,
    ErrorCode::InternalError,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
];

/// The identity a new store was created with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IdentityInitData {
    /// The identity public key (index 0 of the new mnemonic), lowercase hex.
    #[schemars(schema_with = "schema::pubkey_hex")]
    identity_pubkey: String,
}

impl IdentityInitData {
    pub fn new(identity: &PublicKey) -> Self {
        Self {
            identity_pubkey: identity.to_hex(),
        }
    }

    pub fn identity_pubkey(&self) -> &str {
        &self.identity_pubkey
    }
}

/// The identity of the existing store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IdentityShowData {
    /// The identity public key, lowercase hex.
    #[schemars(schema_with = "schema::pubkey_hex")]
    identity_pubkey: String,
    /// The last trade index recorded in the store; `0` before the first trade.
    /// The next order or take uses the one after it.
    #[schemars(schema_with = "schema::trade_index_integer")]
    last_trade_index: i64,
}

impl IdentityShowData {
    /// `last_trade_index` is the value the store holds, already checked to be
    /// in `0..=LAST_TRADE_INDEX` by [`crate::identity::load_identity_from`].
    pub fn new(identity: &PublicKey, last_trade_index: i64) -> Self {
        Self {
            identity_pubkey: identity.to_hex(),
            last_trade_index,
        }
    }

    pub fn identity_pubkey(&self) -> &str {
        &self.identity_pubkey
    }

    pub fn last_trade_index(&self) -> i64 {
        self.last_trade_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LAST_TRADE_INDEX;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::schema::{output_schema, OutcomeRules};
    use crate::outcome::{Envelope, ExecutionMeta, Persistence, Warnings};
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};

    fn key() -> PublicKey {
        Keys::generate().public_key()
    }

    fn init_schema() -> Value {
        Value::Object(output_schema::<IdentityInitData, ()>(&OutcomeRules::local(
            INIT_COMMAND,
            &INIT_ERRORS,
        )))
    }

    fn show_schema() -> Value {
        Value::Object(output_schema::<IdentityShowData, ()>(&OutcomeRules::read(
            SHOW_COMMAND,
            &SHOW_ERRORS,
            false,
        )))
    }

    fn value<D: Serialize>(outcome: Envelope<D, ()>) -> Value {
        serde_json::to_value(outcome).unwrap()
    }

    #[test]
    fn the_data_holds_the_public_key_and_nothing_else() {
        let identity = key();
        let init = serde_json::to_value(IdentityInitData::new(&identity)).unwrap();
        assert_eq!(init, json!({"identity_pubkey": identity.to_hex()}));
        let show = serde_json::to_value(IdentityShowData::new(&identity, 3)).unwrap();
        assert_eq!(
            show,
            json!({"identity_pubkey": identity.to_hex(), "last_trade_index": 3})
        );
    }

    #[test]
    fn every_outcome_of_init_conforms_and_other_stages_do_not() {
        let schema = init_schema();
        let ok = value(Envelope::<_, ()>::ok(
            INIT_COMMAND,
            IdentityInitData::new(&key()),
            Warnings::new(),
            ExecutionMeta::local(Persistence::Applied),
        ));
        assert!(conforms(&schema, &schema, &ok), "{ok}");
        for (code, persistence) in [
            (ErrorCode::IdentityExists, Persistence::NotAttempted),
            (ErrorCode::StoreUnwritable, Persistence::NotAttempted),
            (ErrorCode::StoreUnwritable, Persistence::Failed),
            (ErrorCode::InvalidArguments, Persistence::NotApplicable),
        ] {
            let error = value(Envelope::<IdentityInitData, ()>::error(
                Some(INIT_COMMAND),
                code,
                Warnings::new(),
                ExecutionMeta::local(persistence),
            ));
            assert!(conforms(&schema, &schema, &error), "{error}");
        }
        let mut published = ok.clone();
        published["meta"]["publication"] = json!("published");
        let mut unwritten = ok.clone();
        unwritten["meta"]["persistence"] = json!("not_attempted");
        let mut foreign = value(Envelope::<IdentityInitData, ()>::error(
            Some(INIT_COMMAND),
            ErrorCode::IdentityMissing,
            Warnings::new(),
            ExecutionMeta::local(Persistence::NotAttempted),
        ));
        let mut upper = ok.clone();
        upper["data"]["identity_pubkey"] = json!(key().to_hex().to_uppercase());
        for (label, bad) in [
            ("a publication", &published),
            ("a success that wrote nothing", &unwritten),
            ("a code of show", &foreign),
            ("an uppercase key", &upper),
        ] {
            assert!(!conforms(&schema, &schema, bad), "{label}: {bad}");
        }
        // The data type declares exactly one key.
        assert_eq!(
            schema["$defs"]["IdentityInitData"]["required"],
            json!(["identity_pubkey"])
        );
        foreign["status"] = json!("uncertain");
        assert!(!conforms(&schema, &schema, &foreign), "no uncertain status");
    }

    #[test]
    fn every_outcome_of_show_conforms_and_the_index_is_a_bounded_integer() {
        let schema = show_schema();
        for index in [0, 7, LAST_TRADE_INDEX] {
            let ok = value(Envelope::<_, ()>::ok(
                SHOW_COMMAND,
                IdentityShowData::new(&key(), index),
                Warnings::new(),
                ExecutionMeta::read(None),
            ));
            assert!(conforms(&schema, &schema, &ok), "{ok}");
        }
        for code in [ErrorCode::IdentityMissing, ErrorCode::IdentityUnreadable] {
            let error = value(Envelope::<IdentityShowData, ()>::error(
                Some(SHOW_COMMAND),
                code,
                Warnings::new(),
                ExecutionMeta::read(None),
            ));
            assert!(conforms(&schema, &schema, &error), "{error}");
        }
        let ok = value(Envelope::<_, ()>::ok(
            SHOW_COMMAND,
            IdentityShowData::new(&key(), 1),
            Warnings::new(),
            ExecutionMeta::read(None),
        ));
        for bad_index in [
            json!("1"),
            json!(-1),
            json!(LAST_TRADE_INDEX + 1),
            json!(1.5),
        ] {
            let mut bad = ok.clone();
            bad["data"]["last_trade_index"] = bad_index.clone();
            assert!(!conforms(&schema, &schema, &bad), "{bad_index}");
        }
        let mut written = ok.clone();
        written["meta"]["persistence"] = json!("applied");
        assert!(!conforms(&schema, &schema, &written), "show writes nothing");
    }
}
