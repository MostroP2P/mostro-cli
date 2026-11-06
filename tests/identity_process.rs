//! Tests for `identity init` and `identity show` (spec 009).
//!
//! Every run is the real binary in a sandbox (allow-listed environment, closed
//! stdin, sandboxed HOME and MCLI_DIR). Neither command needs a node or a
//! relay; only the test that uses the new store with `order new` starts the
//! scripted node.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use nostr_sdk::prelude::{Keys, LocalRelay, ToBech32};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{info_event, new_order_daemon, set_last_trade_index, FakeMostro};
use harness::{
    closed_pipe, run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG,
};
use mostro_client::identity::identity_keys_from_mnemonic;

const NODE_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

async fn run(sandbox: &Sandbox, args: &[&str]) -> Captured {
    run_in(sandbox, args, &sandbox.mcli_dir()).await
}

/// Run with `MCLI_DIR` pointing at `dir`.
async fn run_in(sandbox: &Sandbox, args: &[&str], dir: &Path) -> Captured {
    let mut command = sandbox.command(args);
    command.env("MCLI_DIR", dir);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    run
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(
        stdout.ends_with('\n') && stdout.lines().count() == 1,
        "{stdout}"
    );
    serde_json::from_str(&stdout).unwrap()
}

fn meta(publication: &str, acceptance: &str, persistence: &str) -> Value {
    json!({"publication": publication, "daemon_acceptance": acceptance,
           "persistence": persistence, "snapshot": null})
}

fn conforms(command: &str, outcome: &Value) {
    let path = format!(
        "{}/docs/schemas/{}-v1.json",
        env!("CARGO_MANIFEST_DIR"),
        command.replace('.', "-")
    );
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        schema_check::conforms(&schema, &schema, outcome),
        "{outcome}"
    );
}

/// Name and content of every file under `dir`, recursively, sorted.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.push((path.clone(), Vec::new()));
                walk(&path, out);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                out.push((path, bytes));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    files.sort();
    files
}

async fn pool(dir: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(dir.join("mcli.db"))
        .read_only(true);
    SqlitePool::connect_with(options).await.unwrap()
}

/// What a store holds: its schema rows, its user rows (public key, index) and
/// the journal-mode bytes of its header.
async fn describe(
    dir: &Path,
) -> (
    Vec<(String, String, String, Option<String>)>,
    Vec<(String, Option<i64>)>,
    [u8; 2],
) {
    let pool = pool(dir).await;
    let schema =
        sqlx::query_as("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();
    let users = sqlx::query_as("SELECT i0_pubkey, last_trade_index FROM users")
        .fetch_all(&pool)
        .await
        .unwrap();
    pool.close().await;
    let header = std::fs::read(dir.join("mcli.db")).unwrap();
    (schema, users, [header[18], header[19]])
}

/// The secrets of the store in `dir`: the mnemonic and the identity's private
/// key in hex and bech32.
async fn secrets(dir: &Path) -> Vec<String> {
    let pool = pool(dir).await;
    let mnemonic: String = sqlx::query_scalar("SELECT mnemonic FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    let keys = identity_keys_from_mnemonic(&mnemonic).unwrap();
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    vec![
        mnemonic.clone(),
        words[..3].join(" "),
        keys.secret_key().to_secret_hex(),
        keys.secret_key().to_bech32().unwrap(),
    ]
}

fn assert_no_secret(runs: &[&Captured], secrets: &[String]) {
    for run in runs {
        for stream in [run.stdout_text(), run.stderr_text()] {
            for secret in secrets {
                assert!(
                    !stream.contains(secret.as_str()),
                    "a secret leaked: {stream}"
                );
            }
        }
    }
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The store `identity init` creates is the one the legacy first run creates:
/// the same tables, header, file and directory modes, one user with no trade
/// index; and the key reported is the stored one.
#[tokio::test]
async fn init_creates_the_store_the_legacy_first_run_creates() {
    let legacy = Sandbox::new().unwrap();
    let legacy_dir = legacy.root.join("fresh");
    let mut command = legacy.command(["getlasttradeindex"]);
    // The first run creates the store before it needs a relay; an unusable
    // relay list then stops it without any network access.
    command
        .env("MCLI_DIR", &legacy_dir)
        .env("MOSTRO_PUBKEY", NODE_HEX)
        .env("RELAYS", "notarelay");
    let reference = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert_eq!(reference.code(), Some(1), "{}", reference.stderr_text());
    assert!(
        legacy_dir.join("mcli.db").is_file(),
        "the legacy run created no store"
    );

    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.root.join("fresh");
    let run = run_in(&sandbox, &["--output", "json", "identity", "init"], &dir).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let outcome = document(&run);
    let (schema, users, header) = describe(&dir).await;
    let (legacy_schema, legacy_users, legacy_header) = describe(&legacy_dir).await;
    assert_eq!(schema, legacy_schema);
    assert_eq!(header, legacy_header);
    assert_eq!(users.len(), 1);
    assert_eq!(legacy_users.len(), 1);
    assert_eq!(users[0].1, None, "no trade index is recorded");
    assert_eq!(legacy_users[0].1, None);
    assert_eq!(outcome["data"], json!({"identity_pubkey": users[0].0}));
    #[cfg(unix)]
    {
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("mcli.db")), 0o600);
        assert_eq!(mode(&legacy_dir), 0o700);
        assert_eq!(mode(&legacy_dir.join("mcli.db")), 0o600);
    }
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["mcli.db"], "no sidecar or temporary file is left");
}

#[tokio::test]
async fn init_in_json_reports_the_public_key_only_and_needs_no_node() {
    let sandbox = Sandbox::new().unwrap();

    let run = run(
        &sandbox,
        &["--output", "json", "--verbose", "identity", "init"],
    )
    .await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert_eq!(run.stderr_text(), "verbose: identity.init ok\n");
    let outcome = document(&run);
    conforms("identity.init", &outcome);
    assert_eq!(outcome["schema_version"], "1");
    assert_eq!(outcome["command"], "identity.init");
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["error"], Value::Null);
    assert_eq!(outcome["warnings"], json!([]));
    assert_eq!(
        outcome["meta"],
        meta("not_applicable", "not_applicable", "applied")
    );
    let key = outcome["data"]["identity_pubkey"].as_str().unwrap();
    assert!(
        key.len() == 64
            && key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{key}"
    );
    assert_eq!(outcome["data"].as_object().unwrap().len(), 1);
    assert_no_secret(&[&run], &secrets(&sandbox.mcli_dir()).await);
    assert!(harness::is_empty_dir(&sandbox.home()), "HOME was written");
    assert!(
        harness::is_empty_dir(&sandbox.cwd()),
        "the working directory was written"
    );
}

#[tokio::test]
async fn init_in_human_mode_shows_the_public_key_and_never_the_mnemonic() {
    let sandbox = Sandbox::new().unwrap();

    let run = run(&sandbox, &["identity", "init"]).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let (_, users, _) = describe(&sandbox.mcli_dir()).await;
    let out = run.stdout_text();
    assert!(
        out.contains("Identity Created") && out.contains(&users[0].0),
        "{out}"
    );
    assert!(out.contains("never shown"), "{out}");
    assert_no_secret(&[&run], &secrets(&sandbox.mcli_dir()).await);
}

/// A second `identity init`, in either mode, changes no byte.
#[tokio::test]
async fn a_second_init_reports_identity_exists_and_changes_nothing() {
    let sandbox = Sandbox::new().unwrap();
    let first = run(&sandbox, &["--output", "json", "identity", "init"]).await;
    assert_eq!(first.code(), Some(0), "{}", first.stderr_text());
    let before = snapshot(&sandbox.root);

    let json = run(&sandbox, &["--output", "json", "identity", "init"]).await;
    let human = run(&sandbox, &["identity", "init"]).await;

    assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
    assert!(json.stderr.is_empty(), "{}", json.stderr_text());
    let outcome = document(&json);
    conforms("identity.init", &outcome);
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["data"], Value::Null);
    assert_eq!(outcome["error"]["code"], "identity_exists");
    assert_eq!(
        outcome["meta"],
        meta("not_applicable", "not_applicable", "not_attempted")
    );
    assert_eq!(human.code(), Some(1));
    assert!(human.stdout.is_empty(), "{}", human.stdout_text());
    assert_eq!(
        human.stderr_text(),
        "error: A local identity already exists, so nothing was changed.\n"
    );
    assert_eq!(snapshot(&sandbox.root), before, "a file changed");
    assert_no_secret(&[&json, &human], &secrets(&sandbox.mcli_dir()).await);
}

/// A leftover journal or WAL file would be applied to a new store: init
/// treats it as an existing store and creates nothing.
#[tokio::test]
async fn init_refuses_a_leftover_companion_file() {
    for companion in ["mcli.db-journal", "mcli.db-wal", "mcli.db-shm"] {
        let sandbox = Sandbox::new().unwrap();
        std::fs::write(sandbox.mcli_dir().join(companion), b"left over").unwrap();
        let before = snapshot(&sandbox.root);

        let run = run(&sandbox, &["--output", "json", "identity", "init"]).await;

        assert_eq!(run.code(), Some(1), "{companion}");
        assert_eq!(
            document(&run)["error"]["code"],
            "identity_exists",
            "{companion}"
        );
        assert_eq!(snapshot(&sandbox.root), before, "{companion}");
    }
}

/// A directory that cannot be created, or a data path that is a file, is
/// `store_unwritable` with nothing written.
#[tokio::test]
async fn a_directory_that_cannot_be_created_is_store_unwritable() {
    let sandbox = Sandbox::new().unwrap();
    let orphan = sandbox.root.join("no-such-parent").join("mcli");
    let file = sandbox.root.join("a-file");
    std::fs::write(&file, b"not a directory").unwrap();
    for dir in [orphan, file] {
        let before = snapshot(&sandbox.root);

        let run = run_in(&sandbox, &["--output", "json", "identity", "init"], &dir).await;

        assert_eq!(run.code(), Some(1), "{dir:?}: {}", run.stderr_text());
        let outcome = document(&run);
        conforms("identity.init", &outcome);
        assert_eq!(outcome["error"]["code"], "store_unwritable", "{dir:?}");
        assert_eq!(
            outcome["meta"],
            meta("not_applicable", "not_applicable", "not_attempted")
        );
        assert_eq!(snapshot(&sandbox.root), before, "{dir:?}");
    }
}

/// Two runs at once create one store; the other reports `identity_exists`.
#[tokio::test]
async fn two_inits_at_once_create_one_store() {
    let sandbox = Sandbox::new().unwrap();
    let args = ["--output", "json", "identity", "init"];
    let (one, two) = tokio::join!(
        run_with_watchdog(sandbox.command(args), WATCHDOG),
        run_with_watchdog(sandbox.command(args), WATCHDOG)
    );
    let (one, two) = (one.unwrap(), two.unwrap());
    let mut codes = [one.code(), two.code()];
    codes.sort();
    assert_eq!(
        codes,
        [Some(0), Some(1)],
        "{} {}",
        one.stdout_text(),
        two.stdout_text()
    );
    let (winner, loser) = if one.code() == Some(0) {
        (one, two)
    } else {
        (two, one)
    };
    assert_eq!(document(&loser)["error"]["code"], "identity_exists");
    let (_, users, _) = describe(&sandbox.mcli_dir()).await;
    assert_eq!(users.len(), 1);
    assert_eq!(document(&winner)["data"]["identity_pubkey"], users[0].0);
}

#[tokio::test]
async fn show_reports_the_key_and_the_last_trade_index_and_changes_nothing() {
    let sandbox = Sandbox::new().unwrap();
    let init = run(&sandbox, &["--output", "json", "identity", "init"]).await;
    let key = document(&init)["data"]["identity_pubkey"].clone();
    let before = snapshot(&sandbox.root);

    let json = run(
        &sandbox,
        &["--output", "json", "--verbose", "identity", "show"],
    )
    .await;

    assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
    assert_eq!(json.stderr_text(), "verbose: identity.show ok\n");
    let outcome = document(&json);
    conforms("identity.show", &outcome);
    assert_eq!(outcome["command"], "identity.show");
    assert_eq!(
        outcome["data"],
        json!({"identity_pubkey": key, "last_trade_index": 0})
    );
    assert_eq!(
        outcome["meta"],
        meta("not_applicable", "not_applicable", "not_applicable")
    );
    assert_eq!(snapshot(&sandbox.root), before, "show changed a file");

    set_last_trade_index(&sandbox.mcli_dir(), 7).await;
    let json = run(&sandbox, &["--output", "json", "identity", "show"]).await;
    let human = run(&sandbox, &["identity", "show"]).await;
    assert_eq!(document(&json)["data"]["last_trade_index"], 7);
    assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
    let out = human.stdout_text();
    assert!(
        out.contains(key.as_str().unwrap()) && out.contains("Last Trade Index: 7"),
        "{out}"
    );
    assert_no_secret(&[&json, &human], &secrets(&sandbox.mcli_dir()).await);
}

/// Root options are configuration for other commands; identity ignores them,
/// and it needs no Mostro key or relays.
#[tokio::test]
async fn root_options_are_ignored_and_no_node_configuration_is_needed() {
    let sandbox = Sandbox::new().unwrap();
    let init = run(
        &sandbox,
        &[
            "--mostro-pubkey",
            "garbage",
            "--relays",
            "not a relay",
            "--output",
            "json",
            "identity",
            "init",
        ],
    )
    .await;
    assert_eq!(init.code(), Some(0), "{}", init.stderr_text());
    let show = run(
        &sandbox,
        &[
            "--pow",
            "x",
            "--transport",
            "y",
            "--output",
            "json",
            "identity",
            "show",
        ],
    )
    .await;
    assert_eq!(show.code(), Some(0), "{}", show.stderr_text());
}

#[tokio::test]
async fn show_without_a_store_is_identity_missing_and_creates_nothing() {
    let sandbox = Sandbox::new().unwrap();
    let absent = sandbox.root.join("absent");
    for dir in [sandbox.mcli_dir(), absent.clone()] {
        let json = run_in(&sandbox, &["--output", "json", "identity", "show"], &dir).await;
        let human = run_in(&sandbox, &["identity", "show"], &dir).await;

        assert_eq!(json.code(), Some(1), "{dir:?}");
        let outcome = document(&json);
        conforms("identity.show", &outcome);
        assert_eq!(outcome["error"]["code"], "identity_missing");
        assert_eq!(outcome["data"], Value::Null);
        assert_eq!(
            outcome["meta"],
            meta("not_applicable", "not_applicable", "not_applicable")
        );
        assert_eq!(human.code(), Some(1));
        assert_eq!(human.stderr_text(), "error: No local identity exists.\n");
        assert!(human.stdout.is_empty());
    }
    assert!(harness::is_empty_dir(&sandbox.mcli_dir()));
    assert!(!absent.exists());
}

/// Damaged stores are unreadable (or missing without a user row) and are
/// left exactly as they were.
#[tokio::test]
async fn show_on_a_damaged_store_reports_it_and_changes_nothing() {
    let cases: [(&str, &str, &str); 4] = [
        ("garbage", "", "identity_unreadable"),
        ("no user row", "DELETE FROM users", "identity_missing"),
        (
            "null mnemonic",
            "UPDATE users SET mnemonic = NULL",
            "identity_unreadable",
        ),
        (
            "negative index",
            "UPDATE users SET last_trade_index = -1",
            "identity_unreadable",
        ),
    ];
    for (label, sql, code) in cases {
        let sandbox = Sandbox::new().unwrap();
        let init = run(&sandbox, &["--output", "json", "identity", "init"]).await;
        assert_eq!(init.code(), Some(0), "{label}");
        let db = sandbox.mcli_dir().join("mcli.db");
        if sql.is_empty() {
            std::fs::write(&db, b"this is not a sqlite database at all, just text").unwrap();
        } else {
            let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db))
                .await
                .unwrap();
            sqlx::query(sql).execute(&pool).await.unwrap();
            pool.close().await;
        }
        let before = snapshot(&sandbox.root);

        let run = run(&sandbox, &["--output", "json", "identity", "show"]).await;

        assert_eq!(run.code(), Some(1), "{label}");
        let outcome = document(&run);
        conforms("identity.show", &outcome);
        assert_eq!(outcome["error"]["code"], code, "{label}");
        assert_eq!(
            snapshot(&sandbox.root),
            before,
            "{label}: the store changed"
        );
    }
}

#[tokio::test]
async fn options_are_argument_errors_before_any_effect() {
    let sandbox = Sandbox::new().unwrap();
    for (args, command) in [
        (
            &["--output", "json", "identity", "init", "--yes"][..],
            "identity.init",
        ),
        (
            &["--output", "json", "identity", "show", "--reveal"][..],
            "identity.show",
        ),
        (
            &["--output", "json", "identity", "init", "extra"][..],
            "identity.init",
        ),
    ] {
        let run = run(&sandbox, args).await;

        assert_eq!(run.code(), Some(2), "{args:?}");
        let outcome = document(&run);
        conforms(command, &outcome);
        assert_eq!(outcome["command"], command, "{args:?}");
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
        assert_eq!(
            outcome["meta"],
            meta("not_applicable", "not_applicable", "not_applicable")
        );
    }
    harness::assert_no_effects(&sandbox, "identity options");
}

/// The store survives a stdout that cannot be written; the exit says so with
/// one fixed line.
#[tokio::test]
async fn an_unwritable_stdout_exits_1_with_one_fixed_line() {
    for (args, line) in [
        (
            &["--output", "json", "identity", "init"][..],
            "error: the JSON outcome could not be written\n",
        ),
        (
            &["identity", "init"][..],
            "error: the identity could not be written\n",
        ),
    ] {
        let sandbox = Sandbox::new().unwrap();
        let mut command = sandbox.command(args);
        command.stdout(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

        assert_eq!(run.code(), Some(1), "{args:?}");
        assert_eq!(run.stderr_text(), line, "{args:?}");
        assert!(sandbox.mcli_dir().join("mcli.db").is_file(), "{args:?}");
    }
}

/// The store `identity init` creates is ready for the migrated commands:
/// `order new` signs with it against the scripted node at trade index 1.
#[tokio::test]
async fn a_store_from_identity_init_is_used_at_once_by_order_new() {
    let sandbox = Sandbox::new().unwrap();
    let init = run(&sandbox, &["--output", "json", "identity", "init"]).await;
    let identity = document(&init)["data"]["identity_pubkey"].clone();
    let (respond, _) = new_order_daemon(0, false);
    let node = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, node, respond).await;
    let mut command = sandbox.command([
        "--output",
        "json",
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
        "--yes",
    ]);
    command
        .env("MOSTRO_PUBKEY", fake.pubkey().to_hex())
        .env("RELAYS", &relay.url);

    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

    assert_eq!(
        run.code(),
        Some(0),
        "{} {}",
        run.stdout_text(),
        run.stderr_text()
    );
    let outcome = document(&run);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["data"]["trade_index"], "1");
    let received = fake.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].identity.to_hex(), identity.as_str().unwrap());
    let show = self::run(&sandbox, &["--output", "json", "identity", "show"]).await;
    assert_eq!(document(&show)["data"]["last_trade_index"], 1);
}
