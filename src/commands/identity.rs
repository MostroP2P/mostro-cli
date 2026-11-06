//! `identity init` and `identity show`: the local identity store, created or
//! read without any node, relay or network. They return typed outcomes and
//! never print, prompt or exit. Neither ever handles the mnemonic beyond the
//! store layer: `init` gets the new identity's public key back, `show` the key
//! and the recorded trade index.

use std::path::Path;

use nostr_sdk::prelude::PublicKey;

use crate::identity::{load_identity_from, IdentityError};
use crate::outcome::identity::{IdentityInitData, IdentityShowData, INIT_COMMAND, SHOW_COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Persistence, Warnings};
use crate::store::{create_store, CreateError};

pub type InitOutcome = Envelope<IdentityInitData, ()>;
pub type ShowOutcome = Envelope<IdentityShowData, ()>;

/// Create the store in `dir` as the first run does. `None` means no data
/// directory could be determined (no `MCLI_DIR` and no home directory).
pub async fn identity_init(dir: Option<&Path>) -> InitOutcome {
    match dir {
        None => init_failed(ErrorCode::StoreUnwritable, Persistence::NotAttempted),
        Some(dir) => init_outcome(create_store(dir).await),
    }
}

/// The outcome of one attempt to create the store.
fn init_outcome(result: Result<PublicKey, CreateError>) -> InitOutcome {
    match result {
        Ok(identity) => Envelope::ok(
            INIT_COMMAND,
            IdentityInitData::new(&identity),
            Warnings::new(),
            ExecutionMeta::local(Persistence::Applied),
        ),
        Err(CreateError::Exists) => {
            init_failed(ErrorCode::IdentityExists, Persistence::NotAttempted)
        }
        Err(CreateError::NotCreated) => {
            init_failed(ErrorCode::StoreUnwritable, Persistence::NotAttempted)
        }
        Err(CreateError::Failed) => init_failed(ErrorCode::StoreUnwritable, Persistence::Failed),
    }
}

/// Read the identity of the store in `dir`, read-only. `None` means no data
/// directory could be determined, so there is no identity.
pub async fn identity_show(dir: Option<&Path>) -> ShowOutcome {
    let loaded = match dir {
        Some(dir) => load_identity_from(dir).await,
        None => Err(IdentityError::Missing),
    };
    match loaded {
        Ok((keys, index)) => Envelope::ok(
            SHOW_COMMAND,
            IdentityShowData::new(&keys.public_key(), index),
            Warnings::new(),
            ExecutionMeta::read(None),
        ),
        Err(error) => Envelope::error(
            Some(SHOW_COMMAND),
            match error {
                IdentityError::Missing => ErrorCode::IdentityMissing,
                IdentityError::Unreadable => ErrorCode::IdentityUnreadable,
            },
            Warnings::new(),
            ExecutionMeta::read(None),
        ),
    }
}

fn init_failed(code: ErrorCode, persistence: Persistence) -> InitOutcome {
    Envelope::error(
        Some(INIT_COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::local(persistence),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, to_value, Value};

    /// The `status`, `error.code` and `meta.persistence` of an outcome.
    fn row(outcome: InitOutcome) -> Value {
        let value = to_value(outcome).unwrap();
        json!([
            value["status"],
            value["error"]["code"],
            value["meta"]["persistence"]
        ])
    }

    #[test]
    fn each_creation_result_maps_to_its_outcome_row() {
        let identity = Keys::generate().public_key();
        for (result, expected) in [
            (
                Err(CreateError::Exists),
                json!(["error", "identity_exists", "not_attempted"]),
            ),
            (
                Err(CreateError::NotCreated),
                json!(["error", "store_unwritable", "not_attempted"]),
            ),
            (
                Err(CreateError::Failed),
                json!(["error", "store_unwritable", "failed"]),
            ),
            (Ok(identity), json!(["ok", null, "applied"])),
        ] {
            assert_eq!(row(init_outcome(result)), expected, "{result:?}");
        }
    }

    #[tokio::test]
    async fn no_data_directory_is_store_unwritable_for_init_and_identity_missing_for_show() {
        assert_eq!(
            row(identity_init(None).await),
            json!(["error", "store_unwritable", "not_attempted"])
        );
        let show = to_value(identity_show(None).await).unwrap();
        assert_eq!(show["status"], "error");
        assert_eq!(show["error"]["code"], "identity_missing");
        assert_eq!(
            show["meta"],
            json!({
                "publication": "not_applicable",
                "daemon_acceptance": "not_applicable",
                "persistence": "not_applicable",
                "snapshot": null
            })
        );
    }
}
