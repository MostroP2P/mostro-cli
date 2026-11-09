//! Tests for the node settings file `$MCLI_DIR/config` (spec 018): the
//! networked commands, JSON-capable and human-only, read `MOSTRO_PUBKEY`,
//! `RELAYS`, `POW` and `TRANSPORT` from it after the root options and the
//! environment; a file they refuse stops them before any other effect; the
//! local commands never read it.
//!
//! Every run uses the real binary in a sandbox, loopback relays and generated
//! keys; nothing here reaches a real relay or node.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nostr_sdk::prelude::{Keys, LocalRelay, ToBech32};
use serde_json::Value;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{info_event, new_order_daemon, seed_identity, silent, FakeMostro};
use harness::{
    is_empty_dir, run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, SENTINEL, WATCHDOG,
};

const ORDER: &str = "00000000-0000-0000-0000-000000000001";

/// The refusal of a line whose key is not one of the four settings.
const OTHER_KEY: &str = "line 1 sets a key other than MOSTRO_PUBKEY, RELAYS, POW or TRANSPORT";

async fn relay() -> LoopbackRelay {
    LoopbackRelay::start(Backend::Relay(Box::new(LocalRelay::new())))
        .await
        .unwrap()
}

/// Write `text` as the sandbox's config file and return its path.
fn write_config(sandbox: &Sandbox, text: &str) -> PathBuf {
    let path = sandbox.mcli_dir().join("config");
    fs::write(&path, text).unwrap();
    path
}

/// Run `args` in `sandbox` with `env` added to its allow-listed environment.
async fn run(sandbox: &Sandbox, args: &[&str], env: &[(&str, &str)]) -> Captured {
    let mut command = sandbox.command(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    run
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

/// Name and bytes of every entry of `dir`, sorted.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let bytes = fs::read(entry.path()).unwrap_or_default();
            (entry.file_name().to_string_lossy().into_owned(), bytes)
        })
        .collect();
    entries.sort();
    entries
}

/// With no node environment, `order list` takes the key and the relay from a
/// file written with `export`, quotes, a comment and a CRLF line, in both
/// modes, and leaves the data directory as it was.
#[tokio::test]
async fn order_list_reads_the_node_and_relays_from_the_file() {
    let relay = relay().await;
    let node = Keys::generate().public_key().to_hex();
    let sandbox = Sandbox::new().unwrap();
    write_config(
        &sandbox,
        &format!(
            "# the node of this sandbox\n\nexport MOSTRO_PUBKEY=\"{node}\"\r\nexport\tRELAYS='{}'\n",
            relay.url
        ),
    );

    let json = run(&sandbox, &["--output", "json", "order", "list"], &[]).await;
    assert_eq!(json.code(), Some(0), "{}", json.stdout_text());
    let outcome = document(&json);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["data"]["orders"], Value::Array(Vec::new()));
    assert!(json.stderr.is_empty(), "{}", json.stderr_text());
    assert_eq!(relay.connections(), 1);

    let human = run(&sandbox, &["order", "list"], &[]).await;
    assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
    assert_eq!(relay.connections(), 2);

    let names: Vec<String> = snapshot(&sandbox.mcli_dir())
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["config"]);
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));
}

/// With only the file, `order new --yes` reaches the scripted node, and the
/// file's `POW` is the floor of the work mined.
#[tokio::test]
async fn order_new_takes_the_node_relays_and_work_from_the_file() {
    let keys = Keys::generate();
    let local = LocalRelay::new();
    local
        .add_event(info_event(&keys, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(local)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, keys, new_order_daemon(0, false).0).await;
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let node = fake.pubkey().to_hex();
    let text = format!(
        "MOSTRO_PUBKEY={node}\nRELAYS={}\nPOW=8\nTRANSPORT=nip44\n",
        relay.url
    );
    write_config(&sandbox, &text);

    let args = [
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
        "bank",
        "--yes",
    ];
    let run = run(&sandbox, &args, &[]).await;

    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    assert_eq!(document(&run)["status"], "ok");
    let received = fake.received();
    assert_eq!(received.len(), 1);
    assert!(received[0].work >= 8, "mined {} bits", received[0].work);
}

/// The relays come from `-r`, else from a set `RELAYS`, else from the file,
/// counted by the connections each loopback relay accepts. A set but empty
/// variable (or one that is not UTF-8) and an empty option are missing,
/// whatever the file holds.
#[tokio::test]
async fn an_option_beats_a_set_variable_which_beats_the_file() {
    let (a, b, c) = (relay().await, relay().await, relay().await);
    let node = Keys::generate().public_key().to_hex();
    let sandbox = Sandbox::new().unwrap();
    write_config(
        &sandbox,
        &format!("MOSTRO_PUBKEY={node}\nRELAYS={}\n", a.url),
    );
    let list = ["--output", "json", "order", "list"];
    let counts = || [a.connections(), b.connections(), c.connections()];

    let from_file = run(&sandbox, &list, &[]).await;
    assert_eq!(from_file.code(), Some(0), "{}", from_file.stdout_text());
    assert_eq!(counts(), [1, 0, 0]);

    let from_env = run(&sandbox, &list, &[("RELAYS", &b.url)]).await;
    assert_eq!(from_env.code(), Some(0), "{}", from_env.stdout_text());
    assert_eq!(counts(), [1, 1, 0]);

    let option = ["--output", "json", "-r", &c.url, "order", "list"];
    let from_option = run(&sandbox, &option, &[("RELAYS", &b.url)]).await;
    assert_eq!(from_option.code(), Some(0), "{}", from_option.stdout_text());
    assert_eq!(counts(), [1, 1, 1]);

    let empty_variable = run(&sandbox, &list, &[("RELAYS", "")]).await;
    let empty_option = run(
        &sandbox,
        &["--output", "json", "-m", "", "order", "list"],
        &[],
    )
    .await;
    for missing in [empty_variable, empty_option] {
        assert_eq!(missing.code(), Some(1));
        assert_eq!(document(&missing)["error"]["code"], "configuration_missing");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut command = sandbox.command(list);
        command.env("RELAYS", std::ffi::OsStr::from_bytes(b"wss://\xff"));
        let not_utf8 = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(
            document(&not_utf8)["error"]["code"],
            "configuration_missing"
        );
    }
    assert_eq!(counts(), [1, 1, 1], "nothing was connected");
}

/// A file with a key other than the four settings fails every JSON-capable
/// networked command in both modes, even with `-m` and `-r` given: exit 1,
/// `configuration_invalid`, one `hint:` naming the file and the line, the
/// value never shown, no relay contacted and the data directory unchanged.
#[tokio::test]
async fn an_invalid_file_is_configuration_invalid_at_every_entry_point() {
    let relay = relay().await;
    let node = Keys::generate().public_key().to_hex();
    let commands: [&[&str]; 5] = [
        &["order", "list"],
        &["order", "info", "--order", ORDER],
        &[
            "order",
            "new",
            "--side",
            "sell",
            "--currency",
            "USD",
            "--fiat-amount",
            "10",
            "--payment-method",
            "bank",
            "--yes",
        ],
        &["message", "list"],
        &["dispute", "open", "--order", ORDER, "--yes"],
    ];
    for command in commands {
        for json in [true, false] {
            let sandbox = Sandbox::new().unwrap();
            seed_identity(&sandbox.mcli_dir()).await;
            let path = write_config(&sandbox, &format!("ADMIN_NSEC={SENTINEL}\n"));
            let before = snapshot(&sandbox.mcli_dir());
            let mut args = vec!["-m", node.as_str(), "-r", relay.url.as_str()];
            if json {
                args.extend(["--output", "json"]);
            }
            args.extend(command);

            let run = run(&sandbox, &args, &[]).await;

            assert_eq!(run.code(), Some(1), "{args:?}: {}", run.stdout_text());
            let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
            let hint = format!("hint: {}: {OTHER_KEY}", path.display());
            let hints: Vec<&str> = stderr.lines().filter(|l| l.starts_with("hint:")).collect();
            assert_eq!(hints, [hint.as_str()], "{args:?}");
            if json {
                assert_eq!(
                    document(&run)["error"]["code"],
                    "configuration_invalid",
                    "{args:?}"
                );
                assert_eq!(stderr.lines().count(), 1, "{args:?}: {stderr}");
            } else {
                assert!(stdout.is_empty(), "{args:?}: {stdout}");
                assert_eq!(
                    stderr,
                    format!("{hint}\nerror: The node configuration is invalid.\n"),
                    "{args:?}"
                );
            }
            assert!(
                !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
                "{args:?}"
            );
            assert_eq!(snapshot(&sandbox.mcli_dir()), before, "{args:?}");
            assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));
        }
    }
    assert_eq!(relay.connections(), 0, "no relay was contacted");
}

/// `schema`, `identity init`, `identity show` and `order mine` read only the
/// data directory's store: a file they would refuse changes nothing for them.
#[tokio::test]
async fn local_commands_never_read_the_file() {
    let sandbox = Sandbox::new().unwrap();
    write_config(&sandbox, &format!("ADMIN_NSEC={SENTINEL}\n"));
    for command in [
        ["schema"].as_slice(),
        &["identity", "init"],
        &["identity", "show"],
        &["order", "mine"],
    ] {
        let mut args = vec!["--output", "json"];
        args.extend(command);
        let run = run(&sandbox, &args, &[]).await;
        assert_eq!(run.code(), Some(0), "{args:?}: {}", run.stdout_text());
        assert_eq!(document(&run)["status"], "ok", "{args:?}");
        assert!(run.stderr.is_empty(), "{args:?}: {}", run.stderr_text());
    }
}

/// A data directory that does not exist holds no file: the setting is
/// missing, and reading creates nothing.
#[tokio::test]
async fn without_a_data_directory_nothing_is_created() {
    let sandbox = Sandbox::new().unwrap();
    let absent = sandbox.root.join("absent");
    let mut command = sandbox.command(["--output", "json", "order", "list"]);
    command.env("MCLI_DIR", &absent);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert_eq!(run.code(), Some(1));
    assert_eq!(document(&run)["error"]["code"], "configuration_missing");
    assert!(!absent.exists());
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.mcli_dir()));
}

/// `listdisputes`, a human-only command, takes the node and the relays from
/// the file alone; a set `RELAYS` beats the file and `-r` beats both, counted
/// by the connections each loopback relay accepts.
#[tokio::test]
async fn a_legacy_command_reads_the_file_with_the_same_precedence() {
    let (a, b, c) = (relay().await, relay().await, relay().await);
    let node = Keys::generate().public_key().to_hex();
    let sandbox = Sandbox::new().unwrap();
    write_config(
        &sandbox,
        &format!("MOSTRO_PUBKEY={node}\nRELAYS={}\n", a.url),
    );
    let counts = || [a.connections(), b.connections(), c.connections()];

    let from_file = run(&sandbox, &["listdisputes"], &[]).await;
    assert_eq!(from_file.code(), Some(0), "{}", from_file.stderr_text());
    let [file_a, ..] = counts();
    assert!(file_a > 0 && counts()[1..] == [0, 0], "{:?}", counts());

    let from_env = run(&sandbox, &["listdisputes"], &[("RELAYS", &b.url)]).await;
    assert_eq!(from_env.code(), Some(0), "{}", from_env.stderr_text());
    let [env_a, env_b, env_c] = counts();
    assert!(env_a == file_a && env_b > 0 && env_c == 0, "{:?}", counts());

    let option = ["-r", c.url.as_str(), "listdisputes"];
    let from_option = run(&sandbox, &option, &[("RELAYS", &b.url)]).await;
    assert_eq!(from_option.code(), Some(0), "{}", from_option.stderr_text());
    let [option_a, option_b, option_c] = counts();
    assert!(
        option_a == file_a && option_b == env_b && option_c > 0,
        "{:?}",
        counts()
    );
}

/// `admsenddm`, a human-only command that publishes, takes `POW` and
/// `TRANSPORT` from the file alone: the event the node receives carries at
/// least that work, and the transport is explicit, so the node is not probed.
#[tokio::test]
async fn a_legacy_command_takes_the_work_and_transport_from_the_file() {
    let relay = relay().await;
    let fake = FakeMostro::start(&relay.url, Keys::generate(), silent()).await;
    let node = fake.pubkey().to_hex();
    let sandbox = Sandbox::new().unwrap();
    let text = format!(
        "MOSTRO_PUBKEY={node}\nRELAYS={}\nPOW=12\nTRANSPORT=nip44\n",
        relay.url
    );
    write_config(&sandbox, &text);
    let admin = Keys::generate().secret_key().to_bech32().unwrap();

    let args = ["-v", "admsenddm", "--pubkey", &node, "--message", "hi"];
    let run = run(&sandbox, &args, &[("ADMIN_NSEC", &admin)]).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let stderr = run.stderr_text();
    assert!(stderr.contains("Transport: nip44 (explicit)"), "{stderr}");
    for _ in 0..100 {
        if !fake.received().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = fake.received();
    assert_eq!(received.len(), 1);
    assert!(received[0].work >= 12, "mined {} bits", received[0].work);
}

/// A refused file stops a human-only command before `mcli.db` exists or a
/// relay is contacted, even with `-m` and `-r` given, and names the file and
/// the line, never the content.
#[tokio::test]
async fn a_legacy_command_refuses_an_invalid_file_before_creating_the_store() {
    let relay = relay().await;
    let node = Keys::generate().public_key().to_hex();
    let sandbox = Sandbox::new().unwrap();
    let path = write_config(&sandbox, &format!("ADMIN_NSEC={SENTINEL}\n"));

    let args = [
        "-m",
        node.as_str(),
        "-r",
        relay.url.as_str(),
        "listdisputes",
    ];
    let run = run(&sandbox, &args, &[]).await;

    assert_eq!(run.code(), Some(1), "{}", run.stdout_text());
    let stderr = run.stderr_text();
    let expected = format!("Invalid config file {}: {OTHER_KEY}\n", path.display());
    assert_eq!(stderr, expected);
    assert!(!run.stdout_text().contains(SENTINEL) && !stderr.contains(SENTINEL));
    let names: Vec<String> = snapshot(&sandbox.mcli_dir())
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["config"], "no mcli.db");
    assert_eq!(relay.connections(), 0, "no relay was contacted");
}
