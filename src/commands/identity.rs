//! `identity init` and `identity show`: the local identity store, created or
//! read without any node, relay or network. They return typed outcomes and
//! never print, prompt or exit. Neither ever handles the mnemonic beyond the
//! store layer: `init` gets the new identity's public key back, `show` the key
//! and the recorded trade index.

use std::path::Path;

use crate::identity::{load_identity_from, IdentityError};
use crate::outcome::identity::{IdentityInitData, IdentityShowData, INIT_COMMAND, SHOW_COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Persistence, Warnings};
use crate::store::{create_store, CreateError};

pub type InitOutcome = Envelope<IdentityInitData, ()>;
pub type ShowOutcome = Envelope<IdentityShowData, ()>;

/// Create the store in `dir` as the first run does. `None` means no data
/// directory could be determined (no `MCLI_DIR` and no home directory).
pub async fn identity_init(dir: Option<&Path>) -> InitOutcome {
    let Some(dir) = dir else {
        return init_failed(ErrorCode::StoreUnwritable, Persistence::NotAttempted);
    };
    match create_store(dir).await {
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
