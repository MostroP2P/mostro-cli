//! Machine output boundary. An outcome is serialized completely before the
//! first byte reaches stdout, so stdout receives one document or nothing.

use std::io::Write;

use serde::Serialize;

use crate::outcome::{Envelope, RUNTIME_ERROR_EXIT};

/// Stderr text when no outcome could be written.
pub const WRITE_FAILED: &str = "error: the JSON outcome could not be written\n";

/// Write `envelope` to `out` as one JSON line and return the exit status.
///
/// An outcome that cannot be serialized is replaced by an `internal_error`
/// outcome for the same command. A failed write may leave stdout incomplete;
/// nothing is appended after it, a fixed diagnostic goes to `err` and the
/// status is 1.
pub fn emit<D, S>(envelope: &Envelope<D, S>, out: &mut impl Write, err: &mut impl Write) -> i32
where
    D: Serialize,
    S: Serialize,
{
    let (document, exit) = match encode(envelope) {
        Some(document) => (document, envelope.exit_code()),
        None => {
            let fallback = envelope.as_internal_error();
            match encode(&fallback) {
                Some(document) => (document, fallback.exit_code()),
                None => return write_failed(err),
            }
        }
    };
    match out.write_all(&document).and_then(|()| out.flush()) {
        Ok(()) => exit,
        Err(_) => write_failed(err),
    }
}

fn encode<D: Serialize, S: Serialize>(envelope: &Envelope<D, S>) -> Option<Vec<u8>> {
    let mut document = serde_json::to_vec(envelope).ok()?;
    document.push(b'\n');
    Some(document)
}

fn write_failed(err: &mut impl Write) -> i32 {
    let _ = err.write_all(WRITE_FAILED.as_bytes());
    RUNTIME_ERROR_EXIT
}

#[cfg(test)]
mod tests;
