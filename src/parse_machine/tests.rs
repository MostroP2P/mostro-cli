//! Unit tests for the machine output boundary.

use std::io;
use std::str::from_utf8;

use nostr_sdk::prelude::{Keys, Timestamp};
use serde::ser::{Error as _, Serializer};
use serde_json::{json, Value};

use super::*;
use crate::commands::order::list::{ListOutcome, COMMAND};
use crate::outcome::order::{ListData, ListOrdersInput, Snapshot};
use crate::outcome::{ErrorCode, ExecutionMeta, Warnings};

struct Unserializable;

impl Serialize for Unserializable {
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("secret-sentinel"))
    }
}

/// Accepts `limit` bytes, then fails every write like a closed pipe.
struct Truncating {
    written: Vec<u8>,
    limit: usize,
}

impl io::Write for Truncating {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = self.limit - self.written.len();
        if room == 0 {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        let taken = room.min(buf.len());
        self.written.extend_from_slice(&buf[..taken]);
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn documents(bytes: &[u8]) -> Vec<Value> {
    assert!(bytes.ends_with(b"\n"), "missing final newline");
    from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn rejected() -> Envelope<(), ()> {
    Envelope::error(
        None,
        ErrorCode::InvalidArguments,
        Warnings::new(),
        ExecutionMeta::not_executed(),
    )
}

#[test]
fn writes_one_json_line_and_returns_the_outcome_exit() {
    let success = Envelope::<Value, ()>::ok(
        "order.list",
        json!({"orders": []}),
        Warnings::new(),
        ExecutionMeta::read(None),
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(emit(&success, &mut out, &mut err), 0);
    assert_eq!(documents(&out), [serde_json::to_value(&success).unwrap()]);
    assert!(err.is_empty());

    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(emit(&rejected(), &mut out, &mut err), 2);
    let written = documents(&out);
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["command"], Value::Null);
    assert_eq!(written[0]["error"]["code"], json!("invalid_arguments"));
    assert!(err.is_empty());
}

#[test]
fn unserializable_outcome_becomes_one_internal_error() {
    let outcome = Envelope::<Unserializable, ()>::ok(
        "order.list",
        Unserializable,
        Warnings::new(),
        ExecutionMeta::read(None),
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(emit(&outcome, &mut out, &mut err), 1);
    let written = documents(&out);
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["command"], json!("order.list"));
    assert_eq!(written[0]["status"], json!("error"));
    assert_eq!(written[0]["data"], Value::Null);
    assert_eq!(written[0]["error"]["code"], json!("internal_error"));
    assert_eq!(written[0]["meta"]["publication"], json!("not_applicable"));
    assert!(!from_utf8(&out).unwrap().contains("secret-sentinel"));
    assert!(err.is_empty());
}

#[test]
fn failed_write_exits_1_and_appends_nothing() {
    let mut out = Truncating {
        written: Vec::new(),
        limit: 10,
    };
    let mut err = Vec::new();
    assert_eq!(emit(&rejected(), &mut out, &mut err), 1);
    assert_eq!(out.written.len(), 10);
    assert_eq!(err, WRITE_FAILED.as_bytes());
}

fn list_outcome(observed_at: u64) -> ListOutcome {
    let node = Keys::generate().public_key();
    let filters = ListOrdersInput::parse(None, None, None).unwrap();
    let (data, warnings) = ListData::from_selected(&node, filters, &[]).unwrap();
    let snapshot = Snapshot::new(
        Timestamp::from(1_791_504_000),
        Timestamp::from(observed_at),
        50,
        10_000,
        1,
        1,
        0,
    );
    Envelope::ok(COMMAND, data, warnings, ExecutionMeta::read(Some(snapshot)))
}

#[test]
fn list_outcome_is_one_typed_document() {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(emit(&list_outcome(1_792_108_800), &mut out, &mut err), 0);
    let written = documents(&out);
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["command"], json!("order.list"));
    assert_eq!(written[0]["data"]["orders"], json!([]));
    assert_eq!(written[0]["data"]["filters"]["status"], json!("pending"));
    assert_eq!(
        written[0]["meta"]["snapshot"]["observed_at"],
        json!("2026-10-16T00:00:00Z")
    );
    assert!(err.is_empty());
}

#[test]
fn unrepresentable_snapshot_time_becomes_internal_error() {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(emit(&list_outcome(u64::MAX), &mut out, &mut err), 1);
    let written = documents(&out);
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["command"], json!("order.list"));
    assert_eq!(written[0]["data"], Value::Null);
    assert_eq!(written[0]["error"]["code"], json!("internal_error"));
    assert_eq!(written[0]["meta"]["snapshot"], Value::Null);
    assert!(err.is_empty());
}
