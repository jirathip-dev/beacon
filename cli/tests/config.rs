use beacon::config::{Monitor, parse};
use serde_json::{Value, json};
use std::process::Command;

const EXAMPLE: &str = include_str!("../../docs/monitors.example.json");

fn example() -> Value {
    serde_json::from_str(EXAMPLE).unwrap()
}

fn rejects(mutate: impl FnOnce(&mut Value), want: &str) {
    let mut v = example();
    mutate(&mut v);
    let err = parse(&v.to_string()).expect_err(want);
    assert!(err.contains(want), "error {err:?} lacks {want:?}");
}

#[test]
fn example_validates_and_covers_every_monitor_type() {
    let config = parse(EXAMPLE).unwrap();
    let has = |f: fn(&Monitor) -> bool| config.monitors.iter().any(f);
    assert!(has(|m| matches!(m, Monitor::Http(_))));
    assert!(has(|m| matches!(m, Monitor::Tcp(_))));
    assert!(has(|m| matches!(m, Monitor::Heartbeat(_))));
    assert!(config.hosts.values().any(|h| !h.checks.is_empty()));
}

#[test]
fn defaults_fill_omitted_fields() {
    let config = parse(EXAMPLE).unwrap();
    let Monitor::Http(api) = &config.monitors[1] else {
        panic!()
    };
    assert_eq!(api.timeout_s, 10);
    assert_eq!(api.expect_status, None);
    let disk = &config.hosts.values().next().unwrap().checks[1];
    assert_eq!((disk.every_s, disk.timeout_s), (60, 5));
}

#[test]
fn rejects_schema_violations() {
    rejects(|v| v["version"] = json!(2), "unsupported version 2");
    rejects(|v| v["extra"] = json!(1), "unknown field `extra`");
    rejects(
        |v| v["monitors"][0]["timeout"] = json!(5),
        "unknown field `timeout`",
    );
    rejects(
        |v| v["monitors"][0]["type"] = json!("ping"),
        "unknown variant `ping`",
    );
    rejects(
        |v| v["monitors"][0]["url"] = json!("http://example.com/"),
        "want https://",
    );
    rejects(
        |v| v["hosts"]["macmini"]["url"] = json!("http://x/status"),
        "want https://",
    );
    rejects(
        |v| v["monitors"][0]["expect_status"] = json!(99),
        "expect_status 99 out of 100..=599",
    );
    rejects(
        |v| v["monitors"][0]["timeout_s"] = json!(31),
        "timeout_s 31 out of 1..=30",
    );
    rejects(
        |v| v["monitors"][0]["name"] = json!(""),
        "name must be 1..=60 chars",
    );
    rejects(
        |v| v["monitors"][2]["port"] = json!(0),
        "port 0 out of 1..=65535",
    );
    rejects(
        |v| v["monitors"][3]["id"] = json!("Backup"),
        "invalid id \"Backup\"",
    );
    rejects(
        |v| v["hosts"]["macmini"]["checks"][0]["every_s"] = json!(5),
        "every_s 5 out of 10..=86400",
    );
    rejects(
        |v| v["hosts"]["macmini"]["checks"][0]["command"] = json!(""),
        "command must be 1..=",
    );
}

#[test]
fn rejects_cross_field_violations() {
    rejects(
        |v| v["monitors"][3]["id"] = json!("nope"),
        "heartbeat \"Nightly backup\": host \"macmini\" has no check \"nope\"",
    );
    rejects(
        |v| v["monitors"][3]["host"] = json!("other"),
        "host \"other\" has no check",
    );
    rejects(
        |v| v["hosts"]["macmini"]["checks"][1]["id"] = json!("backup"),
        "check \"backup\": duplicate id",
    );
    rejects(
        |v| v["hosts"]["macmini"]["checks"][0]["timeout_s"] = json!(300),
        "timeout_s must be less than every_s",
    );
}

#[test]
fn check_command_exits_0_on_valid_and_2_with_reason_on_invalid() {
    let run = |path: &str| {
        Command::new(env!("CARGO_BIN_EXE_beacon"))
            .args(["check", path])
            .output()
            .unwrap()
    };
    let ok = run(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/monitors.example.json"
    ));
    assert_eq!(ok.status.code(), Some(0));

    let mut v = example();
    v["monitors"][3]["id"] = json!("nope");
    let bad = format!("{}/bad-monitors.json", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&bad, v.to_string()).unwrap();
    let out = run(&bad);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("has no check \"nope\""));
}
