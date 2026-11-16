use beacon::probe;
use beacon::status::Status;
use serde_json::json;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn probe_is_up_on_exit_0_with_last_non_empty_line_of_stdout_and_stderr() {
    let got = probe::run("echo first; echo '  71% used  ' >&2; echo", SECOND * 5);
    assert_eq!(got, (Status::Up, "71% used".into()));
}

#[test]
fn probe_is_down_on_non_zero_exit_and_cuts_message_to_200_chars() {
    let (status, message) = probe::run("printf 'x%.0s' $(seq 300); exit 3", SECOND * 5);
    assert_eq!((status, message.len()), (Status::Down, 200));
}

#[test]
fn probe_timeout_kills_the_whole_process_group() {
    let marker = format!("{}/probe-survivor", env!("CARGO_TARGET_TMPDIR"));
    let _ = std::fs::remove_file(&marker);
    let start = Instant::now();
    let got = probe::run(&format!("(sleep 2; touch {marker}) & sleep 30"), SECOND);
    assert!(start.elapsed() < SECOND * 2, "took {:?}", start.elapsed());
    assert_eq!(got, (Status::Down, "timeout after 1s".into()));
    std::thread::sleep(SECOND * 2);
    assert!(
        !std::path::Path::new(&marker).exists(),
        "grandchild outlived the kill"
    );
}

fn beacon_run(name: &str, checks: serde_json::Value, args: &[&str]) -> Output {
    let config = json!({
        "version": 1,
        "monitors": [],
        "hosts": { "box": { "url": "https://box/status", "checks": checks } }
    });
    let path = format!("{}/{name}.json", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, config.to_string()).unwrap();
    Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args(["run", "--config", &path])
        .args(args)
        .env_remove("BEACON_CONFIG")
        .output()
        .unwrap()
}

#[test]
fn run_prints_each_check_and_exits_0_when_all_up() {
    let checks = json!([
        { "id": "disk", "command": "echo 71% used" },
        { "id": "backup-ok", "command": "true" }
    ]);
    let out = beacon_run("all-up", checks, &["--host", "box"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "disk       up    \"71% used\"\nbackup-ok  up    \"\"\n"
    );
}

#[test]
fn run_exits_1_when_a_check_is_down_or_times_out() {
    let checks = json!([
        { "id": "disk", "command": "echo full; exit 1" },
        { "id": "slow", "command": "sleep 30", "timeout_s": 1, "every_s": 10 }
    ]);
    let out = beacon_run("down", checks, &["--host", "box"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "disk  down  \"full\"\nslow  down  \"timeout after 1s\"\n"
    );
}

#[test]
fn run_exits_2_on_unknown_host_or_bad_usage() {
    let checks = json!([{ "id": "disk", "command": "true" }]);
    let out = beacon_run("unknown-host", checks.clone(), &["--host", "elsewhere"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no host \"elsewhere\" in hosts"));
    let out = beacon_run("bad-usage", checks, &["--host"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: beacon"));
}
