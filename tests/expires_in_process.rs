//! Tests for `order new --expires-in` (spec 012) against the scripted node.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr_sdk::prelude::{Keys, LocalRelay};
use serde_json::Value;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{info_event, new_order_daemon, seed_identity, FakeMostro};
use harness::{run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG};

struct Maker {
    relay: LoopbackRelay,
    fake: FakeMostro,
    sandbox: Sandbox,
}

async fn maker() -> Maker {
    let keys = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&keys, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, keys, new_order_daemon(0, false).0).await;
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    Maker {
        relay,
        fake,
        sandbox,
    }
}

impl Maker {
    async fn run(&self, front: &[&str], tail: &[&str]) -> Captured {
        let mut args: Vec<&str> = front.to_vec();
        args.extend([
            "order",
            "new",
            "--side",
            "sell",
            "--currency",
            "USD",
            "--fiat-amount",
            "10",
            "--payment-method",
            "bank transfer",
        ]);
        args.extend(tail);
        let mut command = self.sandbox.command(&args);
        command
            .env("MOSTRO_PUBKEY", self.fake.pubkey().to_hex())
            .env("RELAYS", &self.relay.url);
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(self.sandbox.mcli_dir())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = std::fs::read(&path).unwrap_or_default();
                (path, bytes)
            })
            .collect();
        files.sort();
        files
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[tokio::test]
async fn expires_in_asks_for_now_plus_the_seconds() {
    let maker = maker().await;
    let before = now();

    let run = maker
        .run(&["--output", "json"], &["--expires-in", "60", "--yes"])
        .await;

    let after = now();
    assert_eq!(
        run.code(),
        Some(0),
        "{} {}",
        run.stdout_text(),
        run.stderr_text()
    );
    let received = maker.fake.received();
    assert_eq!(received.len(), 1);
    let expires_at = received[0].order.as_ref().unwrap().expires_at.unwrap();
    assert!(
        (before + 60..=after + 60).contains(&expires_at),
        "{expires_at} not in {}..={}",
        before + 60,
        after + 60
    );
}

#[tokio::test]
async fn the_human_preview_shows_the_seconds() {
    let maker = maker().await;
    let mut command = maker.sandbox.command([
        "order",
        "new",
        "--side",
        "sell",
        "--currency",
        "USD",
        "--fiat-amount",
        "10",
        "--payment-method",
        "bank transfer",
        "--expires-in",
        "90",
    ]);
    command
        .env("MOSTRO_PUBKEY", maker.fake.pubkey().to_hex())
        .env("RELAYS", &maker.relay.url);
    let run = harness::run_with_input(command, b"n\n".to_vec(), WATCHDOG)
        .await
        .unwrap();
    assert_eq!(run.code(), Some(1), "declined");
    assert!(
        run.stdout_text().contains("Expires in: 90 seconds"),
        "{}",
        run.stdout_text()
    );
    assert!(maker.fake.received().is_empty());
}

/// Out of range, not a number, or together with `--expiration-days`: an
/// argument error in both modes, before anything is sent or written.
#[tokio::test]
async fn invalid_or_conflicting_expiry_is_an_argument_error_before_any_effect() {
    let maker = maker().await;
    let before = maker.snapshot();
    for tail in [
        &["--expires-in", "0"][..],
        &["--expires-in", "31536001"],
        &["--expires-in", "-5"],
        &["--expires-in", "soon"],
        &["--expires-in", "60", "--expiration-days", "2"],
    ] {
        let mut with_yes = tail.to_vec();
        with_yes.push("--yes");

        let json = maker.run(&["--output", "json"], &with_yes).await;
        let human = maker.run(&[], &with_yes).await;

        assert_eq!(json.code(), Some(2), "{tail:?}: {}", json.stdout_text());
        let outcome: Value = serde_json::from_str(&json.stdout_text()).unwrap();
        assert_eq!(outcome["command"], "order.new", "{tail:?}");
        assert_eq!(outcome["error"]["code"], "invalid_arguments", "{tail:?}");
        assert_eq!(human.code(), Some(2), "{tail:?}");
        assert!(human.stdout.is_empty(), "{tail:?}");
    }
    assert!(maker.fake.received().is_empty(), "a request was sent");
    assert_eq!(maker.snapshot(), before, "the store changed");
}

#[tokio::test]
async fn the_limits_are_accepted() {
    for seconds in ["1", "31536000"] {
        let maker = maker().await;
        let run = maker
            .run(&["--output", "json"], &["--expires-in", seconds, "--yes"])
            .await;
        assert_eq!(run.code(), Some(0), "{seconds}: {}", run.stdout_text());
        let sent = maker.fake.received()[0].order.as_ref().unwrap().expires_at;
        assert!(sent.is_some(), "{seconds}");
    }
}
