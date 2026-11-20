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

#[test]
fn disks_default_to_root_and_reject_relative_duplicate_or_too_many() {
    let config = parse(EXAMPLE).unwrap();
    let disks: Vec<_> = config
        .hosts
        .values()
        .next()
        .unwrap()
        .disks
        .iter()
        .map(beacon::config::MountPath::as_str)
        .collect();
    assert_eq!(disks, ["/"]);
    let set = |d: Value| move |v: &mut Value| v["hosts"]["macmini"]["disks"] = d;
    rejects(set(json!(["data"])), "invalid disk");
    rejects(set(json!(["/a\n"])), "invalid disk");
    rejects(set(json!(["/", "/"])), "at most 8 distinct");
    rejects(
        set(json!([
            "/1", "/2", "/3", "/4", "/5", "/6", "/7", "/8", "/9"
        ])),
        "at most 8 distinct",
    );
    let mut v = example();
    set(json!(["/", "/Volumes/backup"]))(&mut v);
    assert_eq!(
        parse(&v.to_string())
            .unwrap()
            .hosts
            .values()
            .next()
            .unwrap()
            .disks
            .len(),
        2
    );
}

#[test]
fn notify_and_rules_parse_with_defaults_and_reject_bad_values() {
    let config = parse(EXAMPLE).unwrap();
    assert_eq!(
        (config.notify.remind_every_h, config.notify.remind_max),
        (4, 3)
    );
    let mut v = example();
    v.as_object_mut().unwrap().remove("notify");
    v["notify"] = json!({ "remind_max": 0 });
    let quiet = parse(&v.to_string()).unwrap().notify;
    assert_eq!((quiet.remind_every_h, quiet.remind_max), (4, 0));
    v["hosts"]["macmini"]["rules"] = json!([]);
    let host = parse(&v.to_string()).unwrap();
    assert_eq!(host.hosts.values().next().unwrap().rules.len(), 0);
    let heartbeat = json!({ "type": "heartbeat", "name": "Disk full", "group": "macmini", "host": "macmini", "id": "disk-full" });
    let mut v = example();
    v["monitors"].as_array_mut().unwrap().push(heartbeat);
    parse(&v.to_string()).unwrap();
    let rule = |r: Value| move |v: &mut Value| v["hosts"]["macmini"]["rules"] = json!([r]);
    rejects(
        rule(json!({ "id": "x", "metric": "swap", "above": 1, "for_min": 1 })),
        "unknown variant `swap`",
    );
    rejects(
        rule(json!({ "id": "x", "metric": "disk", "above": 101, "for_min": 1 })),
        "rule \"x\": above 101 out of 0..=100",
    );
    rejects(
        rule(json!({ "id": "x", "metric": "load", "above": 2, "for_min": 0 })),
        "for_min 0 out of 1..=1440",
    );
    rejects(
        rule(json!({ "id": "backup", "metric": "memory", "above": 2, "for_min": 1 })),
        "rule check \"backup\" reuses an id",
    );
    rejects(
        |v| v["notify"] = json!({ "remind_every_h": 0 }),
        "notify.remind_every_h 0 out of 1..=168",
    );
    rejects(
        |v| v["notify"] = json!({ "every": 1 }),
        "unknown field `every`",
    );
}

#[test]
fn jobs_block_parses_and_rejects_bad_labels_reviews_and_reserved_ids() {
    use beacon::config::{Kept, Review};
    let config = parse(EXAMPLE).unwrap();
    let jobs = &config.hosts.values().next().unwrap().jobs;
    assert_eq!(jobs.get("com.example.caffeinate"), Some(&Review::Ignore));
    assert_eq!(jobs.get("dev.example.old"), Some(&Review::Removed));
    let kept = Kept {
        name: "tick".into(),
        group: "macmini".into(),
        running: false,
    };
    assert_eq!(jobs.get("dev.example.tick"), Some(&Review::Keep(kept)));
    let job = |label: &'static str, review: Value| {
        move |v: &mut Value| v["hosts"]["macmini"]["jobs"][label] = review
    };
    rejects(job("a b", json!("ignore")), "invalid job label \"a b\"");
    rejects(job("x", json!("forget")), "unknown variant `forget`");
    let long = "n".repeat(61);
    rejects(
        job("x", json!({ "keep": { "name": long, "group": "g" } })),
        "job \"x\": name must be 1..=60 chars",
    );
    rejects(
        job(
            "x",
            json!({ "keep": { "name": "n", "group": "g", "when": 1 } }),
        ),
        "unknown field `when`",
    );
    rejects(
        |v| v["hosts"]["macmini"]["checks"][0]["id"] = json!("job-backup"),
        "reserved for jobs",
    );
    let mut v = example();
    job(
        "x",
        json!({ "keep": { "name": "n", "group": "g", "running": true } }),
    )(&mut v);
    let config = parse(&v.to_string()).unwrap();
    let running = &config.hosts.values().next().unwrap().jobs;
    assert!(matches!(running.get("x"), Some(Review::Keep(k)) if k.running));
}
