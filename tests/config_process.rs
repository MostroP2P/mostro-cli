//! Tests for `config set` and `config show` (spec 018): the real binary writes
//! the node settings to `$MCLI_DIR/config` and reports what the file holds,
//! with no node, relay, stdin or confirmation, refuses values and files
//! without echoing them, and what it writes is what the networked commands
//! read.
//!
//! Every run uses the real binary in a sandbox; the only relays are loopback
//! relays, and nothing here reaches a real relay or node.

use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use nostr_sdk::prelude::{Keys, LocalRelay, ToBech32};
use serde_json::{json, Value};

#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use harness::{
    closed_pipe, is_empty_dir, run_with_open_stdin, run_with_watchdog, set_node_env, Backend,
    Captured, LoopbackRelay, Sandbox, SENTINEL, WATCHDOG,
};

/// The refusal of a line whose key is not one of the four settings.
const OTHER_KEY: &str = "sets a key other than MOSTRO_PUBKEY, RELAYS, POW or TRANSPORT";

async fn relay() -> LoopbackRelay {
    LoopbackRelay::start(Backend::Relay(Box::new(LocalRelay::new())))
        .await
        .unwrap()
}

/// Run `args` in `sandbox` with `MCLI_DIR` at `dir`.
async fn run_in(sandbox: &Sandbox, dir: &Path, args: &[&str]) -> Captured {
    let mut command = sandbox.command(args);
    command.env("MCLI_DIR", dir);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    run
}

/// `config set key value` in JSON with `MCLI_DIR` at `dir`.
async fn set(sandbox: &Sandbox, dir: &Path, key: &str, value: &str) -> Captured {
    run_in(
        sandbox,
        dir,
        &["--output", "json", "config", "set", key, value],
    )
    .await
}

/// `config show` in JSON with `MCLI_DIR` at `dir`.
async fn show(sandbox: &Sandbox, dir: &Path) -> Captured {
    run_in(sandbox, dir, &["--output", "json", "config", "show"]).await
}

/// The one JSON document of `config set` on stdout.
fn document(run: &Captured) -> Value {
    document_of(run, "config.set")
}

/// The one JSON document of `command` on stdout, which conforms to its
/// checked-in schema.
fn document_of(run: &Captured, command: &str) -> Value {
    let stdout = run.stdout_text();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let outcome: Value = serde_json::from_str(&stdout).unwrap();
    let path = format!(
        "{}/docs/schemas/{}-v1.json",
        env!("CARGO_MANIFEST_DIR"),
        command.replace('.', "-")
    );
    let schema: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        schema_check::conforms(&schema, &schema, &outcome),
        "{outcome}"
    );
    assert_eq!(outcome["command"], command);
    outcome
}

fn meta(persistence: &str) -> Value {
    json!({"publication": "not_applicable", "daemon_acceptance": "not_applicable",
           "persistence": persistence, "snapshot": null})
}

/// The names of the entries of `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A data directory that does not exist yet becomes owner-only, the file too,
/// and it holds only the file and its lock: no store, no temporary file.
#[tokio::test]
async fn config_set_creates_an_owner_only_file() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.root.join("fresh");
    let node = Keys::generate().public_key().to_bech32().unwrap();

    let run = set(&sandbox, &dir, "mostro-pubkey", &node).await;

    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["meta"], meta("applied"));
    let path = dir.join("config");
    assert_eq!(
        outcome["data"],
        json!({"path": path.to_str().unwrap(), "mostro_pubkey": node, "relays": null,
               "pow": null, "transport": null})
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("MOSTRO_PUBKEY={node}\n")
    );
    assert_eq!(names(&dir), [".config.lock", "config"]);
    #[cfg(unix)]
    {
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&path), 0o600);
    }
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));
}

/// A hand-written file keeps the other settings' values and is rewritten as
/// plain `NAME=value` lines: no comment, `export`, quotes, byte order mark or
/// repeated line survives. The value is trimmed.
#[tokio::test]
async fn config_set_replaces_one_value_and_keeps_the_others() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let node = Keys::generate().public_key().to_hex();
    fs::write(
        dir.join("config"),
        format!("\u{feff}# my node\nexport MOSTRO_PUBKEY=\"{node}\"\r\nPOW='3'\nPOW=4\n"),
    )
    .unwrap();

    let run = set(
        &sandbox,
        &dir,
        "relays",
        " wss://a.example , wss://b.example ",
    )
    .await;

    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    let data = document(&run)["data"].clone();
    assert_eq!(data["mostro_pubkey"], node.as_str());
    assert_eq!(data["relays"], "wss://a.example , wss://b.example");
    assert_eq!(data["pow"], "4");
    assert_eq!(
        fs::read_to_string(dir.join("config")).unwrap(),
        format!("MOSTRO_PUBKEY={node}\nRELAYS=wss://a.example , wss://b.example\nPOW=4\n")
    );
}

/// A value its setting refuses is `invalid_arguments` (exit 2) in both modes,
/// never echoed: a missing data directory stays missing and an existing file
/// keeps its bytes.
#[tokio::test]
async fn refused_values_are_invalid_arguments_and_write_nothing() {
    let sandbox = Sandbox::new().unwrap();
    let absent = sandbox.root.join("absent");
    let dir = sandbox.mcli_dir();
    fs::write(dir.join("config"), b"POW=1\n").unwrap();
    let nsec = Keys::generate().secret_key().to_bech32().unwrap();
    let rows = [
        ("mostro-pubkey", nsec),
        ("relays", format!("https://{SENTINEL}.example")),
        ("pow", "256".to_string()),
        ("transport", "gift-wrap".to_string()),
        ("relays", String::new()),
        ("relays", format!("wss://a.example\n{SENTINEL}")),
    ];
    for (key, value) in &rows {
        let json = set(&sandbox, &absent, key, value).await;
        let human = run_in(&sandbox, &dir, &["config", "set", key, value]).await;

        for run in [&json, &human] {
            assert_eq!(run.code(), Some(2), "{key}");
            let text = run.stdout_text() + &run.stderr_text();
            assert!(value.is_empty() || !text.contains(value.as_str()), "{text}");
            assert!(!text.contains(SENTINEL), "{text}");
        }
        let outcome = document(&json);
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
        assert_eq!(outcome["meta"], meta("not_applicable"));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: The command arguments are invalid.\n"
        );
    }
    assert!(!absent.exists());
    assert_eq!(names(&dir), ["config"]);
    assert_eq!(fs::read(dir.join("config")).unwrap(), b"POW=1\n");
}

/// A key that is not one of the four is an argument error resolved to
/// `config.set`, and the value is not echoed.
#[tokio::test]
async fn an_unknown_key_is_an_argument_error_that_never_echoes_the_value() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    for output in ["json", "human"] {
        let args = ["--output", output, "config", "set", "admin-nsec", SENTINEL];
        let run = run_in(&sandbox, &dir, &args).await;

        assert_eq!(run.code(), Some(2), "{output}");
        let text = run.stdout_text() + &run.stderr_text();
        assert!(!text.contains(SENTINEL), "{text}");
        if output == "json" {
            let outcome = document(&run);
            assert_eq!(outcome["error"]["code"], "invalid_arguments");
            assert_eq!(outcome["meta"], meta("not_applicable"));
        }
    }
    assert!(is_empty_dir(&dir));
}

/// An existing file the commands refuse is never rewritten: one `hint:` line
/// names the file and the line, and nothing is created beside it.
#[tokio::test]
async fn an_existing_invalid_file_is_left_untouched() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let path = dir.join("config");
    let bytes = format!("POW=1\nADMIN_NSEC={SENTINEL}\n");
    fs::write(&path, &bytes).unwrap();
    let hint = format!("hint: {}: line 2 {OTHER_KEY}\n", path.display());

    let json = set(&sandbox, &dir, "pow", "2").await;
    let human = run_in(&sandbox, &dir, &["config", "set", "pow", "2"]).await;

    for run in [&json, &human] {
        assert_eq!(run.code(), Some(1));
        let text = run.stdout_text() + &run.stderr_text();
        assert!(!text.contains(SENTINEL), "{text}");
    }
    let outcome = document(&json);
    assert_eq!(outcome["error"]["code"], "configuration_invalid");
    assert_eq!(outcome["meta"], meta("not_attempted"));
    assert_eq!(json.stderr_text(), hint);
    assert!(human.stdout.is_empty());
    assert_eq!(
        human.stderr_text(),
        format!("{hint}error: The node configuration is invalid.\n")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    assert_eq!(names(&dir), ["config"]);
}

/// A data directory the process may not search is refused (strict read): its
/// mode is not changed to 0700 and the file it holds is not overwritten.
#[cfg(unix)]
#[tokio::test]
async fn an_unreachable_data_directory_is_refused_and_left_untouched() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let path = dir.join("config");
    let bytes = b"POW=1\nTRANSPORT=nip44\n";
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o600)).unwrap();
    if fs::metadata(&path).is_ok() {
        // Root reaches the file whatever the mode; the case needs a user it stops.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }

    let run = set(&sandbox, &dir, "pow", "2").await;
    let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(run.code(), Some(1));
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], "configuration_invalid");
    assert_eq!(outcome["meta"], meta("not_attempted"));
    assert_eq!(
        run.stderr_text(),
        format!("hint: {}: not a readable UTF-8 text file\n", path.display())
    );
    assert_eq!(mode, 0o600);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

/// A data directory that is a regular file, or one whose parent does not
/// exist, cannot be written: nothing is created.
#[tokio::test]
async fn an_unusable_data_directory_is_configuration_unwritable() {
    let sandbox = Sandbox::new().unwrap();
    let file = sandbox.root.join("file");
    fs::write(&file, b"not a directory").unwrap();
    let deep = sandbox.root.join("absent").join("child");
    for dir in [&file, &deep] {
        let run = set(&sandbox, dir, "pow", "2").await;

        assert_eq!(run.code(), Some(1), "{dir:?}");
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "configuration_unwritable");
        assert_eq!(outcome["meta"], meta("not_attempted"));
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    }
    assert_eq!(fs::read(&file).unwrap(), b"not a directory");
    assert!(!sandbox.root.join("absent").exists());
}

/// Human mode lists the path and the four values, `--verbose` adds one line,
/// and a stdout that cannot be written exits 1 with one fixed line after the
/// file was written.
#[tokio::test]
async fn human_mode_verbose_and_a_closed_stdout() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let human = run_in(
        &sandbox,
        &dir,
        &["--verbose", "config", "set", "transport", "nip44"],
    )
    .await;

    assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
    assert_eq!(
        human.stdout_text(),
        format!(
            "Config file: {}\nMOSTRO_PUBKEY: not set\nRELAYS: not set\nPOW: not set\n\
             TRANSPORT: nip44\n",
            dir.join("config").display()
        )
    );
    assert_eq!(human.stderr_text(), "verbose: config.set ok\n");

    for (args, line) in [
        (
            &["--output", "json", "config", "set", "pow", "3"][..],
            "error: the JSON outcome could not be written\n",
        ),
        (
            &["config", "set", "pow", "4"][..],
            "error: the config set result could not be written\n",
        ),
    ] {
        let mut command = sandbox.command(args);
        command.stdout(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1), "{args:?}");
        assert_eq!(run.stderr_text(), line, "{args:?}");
    }
    assert_eq!(
        fs::read_to_string(dir.join("config")).unwrap(),
        "POW=4\nTRANSPORT=nip44\n"
    );
}

/// `config set` connects to nothing, even with a node configured, needs no
/// `--yes` and never waits for stdin.
#[tokio::test]
async fn config_set_needs_no_node_stdin_or_yes() {
    let relay = relay().await;
    let sandbox = Sandbox::new().unwrap();
    let mut command = sandbox.command(["--output", "json", "config", "set", "pow", "5"]);
    set_node_env(&mut command, &Keys::generate(), &[&relay]);

    let run = run_with_open_stdin(command, WATCHDOG).await.unwrap();

    assert!(run.reaped && run.status.is_some());
    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    assert_eq!(document(&run)["data"]["pow"], "5");
    assert_eq!(relay.connections(), 0);
}

/// `config show` reports the file's path and the values stored in it, an
/// absent one as null, in both modes, and changes nothing.
#[tokio::test]
async fn config_show_reports_the_file_path_and_its_values() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let path = dir.join("config");
    let node = Keys::generate().public_key().to_hex();
    let bytes = format!("# my node\nexport MOSTRO_PUBKEY='{node}'\r\nPOW=7\n");
    fs::write(&path, &bytes).unwrap();

    let json = show(&sandbox, &dir).await;
    let human = run_in(&sandbox, &dir, &["config", "show"]).await;

    assert_eq!(json.code(), Some(0), "{}", json.stdout_text());
    assert!(json.stderr.is_empty(), "{}", json.stderr_text());
    let outcome = document_of(&json, "config.show");
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["meta"], meta("not_applicable"));
    assert_eq!(
        outcome["data"],
        json!({"path": path.to_str().unwrap(), "mostro_pubkey": node, "relays": null,
               "pow": "7", "transport": null})
    );
    assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
    assert_eq!(
        human.stdout_text(),
        format!(
            "Config file: {}\nMOSTRO_PUBKEY: {node}\nRELAYS: not set\nPOW: 7\nTRANSPORT: not set\n",
            path.display()
        )
    );
    assert!(human.stderr.is_empty(), "{}", human.stderr_text());
    assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    assert_eq!(names(&dir), ["config"]);
}

/// Without a file the path is still given and every value is null; a data
/// directory that does not exist is not created.
#[tokio::test]
async fn config_show_without_a_file_creates_nothing() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.root.join("absent");

    let run = show(&sandbox, &dir).await;

    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    assert_eq!(
        document_of(&run, "config.show")["data"],
        json!({"path": dir.join("config").to_str().unwrap(), "mostro_pubkey": null,
               "relays": null, "pow": null, "transport": null})
    );
    assert!(!dir.exists());
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));
}

/// Options and environment are not reported, only the file; nothing is
/// contacted and stdin is never read.
#[tokio::test]
async fn config_show_reports_only_the_file() {
    let relay = relay().await;
    let sandbox = Sandbox::new().unwrap();
    fs::write(sandbox.mcli_dir().join("config"), "POW=1\n").unwrap();
    let args = [
        "--output", "json", "-r", &relay.url, "-p", "9", "config", "show",
    ];
    let mut command = sandbox.command(args);
    set_node_env(&mut command, &Keys::generate(), &[&relay]);

    let run = run_with_open_stdin(command, WATCHDOG).await.unwrap();

    assert!(run.reaped && run.status.is_some());
    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    let data = document_of(&run, "config.show")["data"].clone();
    let values = [&data["mostro_pubkey"], &data["relays"], &data["pow"]];
    assert_eq!(values, [&Value::Null, &Value::Null, &json!("1")]);
    assert_eq!(relay.connections(), 0);
}

/// A file the commands refuse is `configuration_invalid` in both modes, with
/// one `hint:` naming the file and the line, and nothing echoed.
#[tokio::test]
async fn config_show_refuses_an_invalid_file_without_echoing_it() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let path = dir.join("config");
    fs::write(&path, format!("ADMIN_NSEC={SENTINEL}\nPOW=1\n")).unwrap();
    let hint = format!("hint: {}: line 1 {OTHER_KEY}\n", path.display());

    let json = show(&sandbox, &dir).await;
    let human = run_in(&sandbox, &dir, &["config", "show"]).await;

    for run in [&json, &human] {
        assert_eq!(run.code(), Some(1));
        let text = run.stdout_text() + &run.stderr_text();
        assert!(!text.contains(SENTINEL), "{text}");
    }
    let outcome = document_of(&json, "config.show");
    assert_eq!(outcome["error"]["code"], "configuration_invalid");
    assert_eq!(outcome["meta"], meta("not_applicable"));
    assert_eq!(json.stderr_text(), hint);
    assert!(human.stdout.is_empty());
    assert_eq!(
        human.stderr_text(),
        format!("{hint}error: The node configuration is invalid.\n")
    );
    assert_eq!(names(&dir), ["config"]);
}

/// A data directory the process may not search is refused (strict read)
/// rather than reported as a file holding nothing.
#[cfg(unix)]
#[tokio::test]
async fn config_show_refuses_an_unreachable_data_directory() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let path = dir.join("config");
    fs::write(&path, b"POW=1\n").unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o600)).unwrap();
    if fs::metadata(&path).is_ok() {
        // Root reaches the file whatever the mode; the case needs a user it stops.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }

    let run = show(&sandbox, &dir).await;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(run.code(), Some(1));
    let outcome = document_of(&run, "config.show");
    assert_eq!(outcome["error"]["code"], "configuration_invalid");
    assert_eq!(
        run.stderr_text(),
        format!("hint: {}: not a readable UTF-8 text file\n", path.display())
    );
}

/// A control character in the data directory's path is escaped on the
/// `hint:` line of both commands, as on the `Config file:` line.
#[cfg(unix)]
#[tokio::test]
async fn the_hint_escapes_control_characters_in_the_path() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir().with_file_name("mcli\u{1b}[31m");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("config");
    fs::write(&path, b"ADMIN_NSEC=x\n").unwrap();
    let shown = path.display().to_string().replace('\u{1b}', "\\u{1b}");
    let hint = format!("hint: {shown}: line 1 {OTHER_KEY}\n");

    for args in [&["config", "show"][..], &["config", "set", "pow", "1"]] {
        let run = run_in(&sandbox, &dir, args).await;
        assert_eq!(run.code(), Some(1), "{args:?}");
        let invalid = "error: The node configuration is invalid.\n";
        assert_eq!(run.stderr_text(), format!("{hint}{invalid}"), "{args:?}");
    }
}

/// What `config set` writes is what `config show` reports and what
/// `order list` reads with no node environment (SC-004).
#[tokio::test]
async fn config_set_round_trips_with_config_show_and_order_list() {
    let relay = relay().await;
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let node = Keys::generate().public_key().to_bech32().unwrap();

    let first = set(&sandbox, &dir, "mostro-pubkey", &node).await;
    let second = set(&sandbox, &dir, "relays", &relay.url).await;
    let shown = show(&sandbox, &dir).await;
    let list = run_in(&sandbox, &dir, &["--output", "json", "order", "list"]).await;

    for run in [&first, &second, &shown, &list] {
        assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    }
    let written = document(&second)["data"].clone();
    assert_eq!(written["mostro_pubkey"], node.as_str());
    assert_eq!(document_of(&shown, "config.show")["data"], written);
    let listed: Value = serde_json::from_str(&list.stdout_text()).unwrap();
    assert_eq!(listed["status"], "ok");
    assert_eq!(relay.connections(), 1);
}

/// `--verbose` adds one line, and a stdout that cannot be written exits 1
/// with one fixed line.
#[tokio::test]
async fn config_show_verbose_and_a_closed_stdout() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let verbose = run_in(
        &sandbox,
        &dir,
        &["--verbose", "--output", "json", "config", "show"],
    )
    .await;

    assert_eq!(verbose.code(), Some(0), "{}", verbose.stderr_text());
    assert_eq!(document_of(&verbose, "config.show")["status"], "ok");
    assert_eq!(verbose.stderr_text(), "verbose: config.show ok\n");

    for (args, line) in [
        (
            &["--output", "json", "config", "show"][..],
            "error: the JSON outcome could not be written\n",
        ),
        (
            &["config", "show"][..],
            "error: the config show result could not be written\n",
        ),
    ] {
        let mut command = sandbox.command(args);
        command.stdout(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1), "{args:?}");
        assert_eq!(run.stderr_text(), line, "{args:?}");
    }
    assert!(is_empty_dir(&dir));
}
