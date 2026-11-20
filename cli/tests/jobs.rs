use beacon::config::{Label, Review};
use beacon::jobs::{Detail, Kind, Row, Watcher, launchd, systemd};
use beacon::status::Status;
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/jobs");

fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!("{FIX}/{path}")).unwrap()
}

fn reviews(v: Value) -> BTreeMap<Label, Review> {
    serde_json::from_value(v).unwrap()
}

fn print(label: &str) -> Option<Detail> {
    let text = std::fs::read_to_string(format!("{FIX}/launchd/print/{label}")).ok()?;
    launchd::detail(&text)
}

fn rows(listing: &str, r: &BTreeMap<Label, Review>) -> BTreeMap<String, Row> {
    let listing = launchd::listing(listing);
    let rows = Watcher::default().update(&listing, r, Instant::now(), print);
    rows.into_iter().map(|r| (r.label.clone(), r)).collect()
}

fn verdict(rows: &BTreeMap<String, Row>, label: &str) -> (Status, String) {
    let r = &rows[label];
    (r.status, r.message.clone())
}

fn up(m: &str) -> (Status, String) {
    (Status::Up, m.into())
}

fn down(m: &str) -> (Status, String) {
    (Status::Down, m.into())
}

#[test]
fn launchd_listing_leaves_out_apple_application_and_beacon_and_reads_pid_and_status() {
    let listing = launchd::listing(&fixture("launchd/list"));
    let labels: Vec<_> = listing.iter().map(|j| j.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "dev.shed.serve",
            "dev.cooee.cooee",
            "dev.drover.tick",
            "dev.drover.clean",
            "com.jirathip.boat-rt",
            "com.jirathip.ttyd",
            "actions.runner.jirathip-dev-shed.shed-mini",
            "com.jirathip.caffeinate",
        ]
    );
    assert!(listing[0].running);
    assert_eq!(listing[0].failed.as_deref(), Some("last exit signal 9"));
    assert!(!listing[1].running);
    assert_eq!(listing[1].failed, None);
    assert_eq!(listing[5].failed.as_deref(), Some("last exit 1"));
}

#[test]
fn launchd_print_gives_keepalive_interval_calendar_and_on_demand() {
    let kind = |l: &str| print(l).unwrap().kind;
    assert_eq!(kind("dev.shed.serve"), Kind::KeepAlive);
    assert_eq!(kind("dev.drover.tick"), Kind::Scheduled(300));
    assert_eq!(kind("dev.drover.clean"), Kind::Scheduled(86_400));
    assert_eq!(
        kind("actions.runner.jirathip-dev-shed.shed-mini"),
        Kind::OnDemand
    );
    assert_eq!(print("dev.drover.tick").unwrap().runs, "170");
    assert_eq!(launchd::detail("Could not find service"), None);
}

#[test]
fn keepalive_is_up_while_running_and_down_when_not() {
    let r = rows(&fixture("launchd/list"), &BTreeMap::new());
    assert_eq!(verdict(&r, "dev.shed.serve"), up("running"));
    assert_eq!(verdict(&r, "com.jirathip.ttyd"), up("running"));
    assert_eq!(verdict(&r, "dev.cooee.cooee"), down("not running"));
    let stopped = fixture("launchd/list").replace("50226\t-9\tdev.shed", "-\t-9\tdev.shed");
    let r = rows(&stopped, &BTreeMap::new());
    assert_eq!(
        verdict(&r, "dev.shed.serve"),
        down("not running, last exit signal 9")
    );
}

#[test]
fn a_scheduled_job_whose_last_exit_failed_is_down() {
    let failing =
        fixture("launchd/list").replace("-\t0\tcom.jirathip.boat-rt", "-\t1\tcom.jirathip.boat-rt");
    let r = rows(&failing, &BTreeMap::new());
    assert_eq!(verdict(&r, "com.jirathip.boat-rt"), down("last exit 1"));
    assert_eq!(verdict(&r, "dev.drover.clean"), down("last exit 1"));
    assert_eq!(verdict(&r, "dev.drover.tick"), up("running"));
    let r = rows(&fixture("launchd/list"), &BTreeMap::new());
    assert_eq!(verdict(&r, "com.jirathip.boat-rt"), up("last exit 0"));
}

#[test]
fn a_scheduled_job_that_misses_2_intervals_is_down_until_it_runs_again() {
    let listing = launchd::listing(&fixture("launchd/list"));
    let (ran, reads) = (Cell::new(false), Cell::new(0));
    let read = |l: &str| {
        let mut d = print(l)?;
        if l == "com.jirathip.boat-rt" {
            reads.set(reads.get() + 1);
            if ran.get() {
                d.runs = "42".into();
            }
        }
        Some(d)
    };
    let (mut w, t0, none) = (Watcher::default(), Instant::now(), BTreeMap::new());
    let at = |w: &mut Watcher, s: u64| {
        let rows = w.update(&listing, &none, t0 + Duration::from_secs(s), read);
        let r = rows.into_iter().find(|r| r.label == "com.jirathip.boat-rt");
        r.map(|r| (r.status, r.message)).unwrap()
    };
    assert_eq!(at(&mut w, 0), up("last exit 0"));
    assert_eq!(at(&mut w, 30), up("last exit 0"));
    assert_eq!(reads.get(), 1, "no read before the interval is up");
    assert_eq!(at(&mut w, 60), up("last exit 0"));
    assert_eq!(at(&mut w, 120), down("no run in 2 intervals of 60s"));
    assert_eq!(reads.get(), 3);
    ran.set(true);
    assert_eq!(at(&mut w, 180), up("last exit 0"));
}

#[test]
fn an_on_demand_job_is_down_only_when_stopped_after_a_failure_unless_kept_running() {
    let runner = "actions.runner.jirathip-dev-shed.shed-mini";
    let stopped = fixture("launchd/list").replace("734\t0\tactions", "-\t0\tactions");
    assert_eq!(
        verdict(&rows(&fixture("launchd/list"), &BTreeMap::new()), runner),
        up("running")
    );
    assert_eq!(
        verdict(&rows(&stopped, &BTreeMap::new()), runner),
        up("last exit 0")
    );
    let failed = stopped.replace("-\t0\tactions", "-\t2\tactions");
    let r = rows(&failed, &BTreeMap::new());
    assert_eq!(verdict(&r, runner), down("not running, last exit 2"));
    let must = reviews(
        json!({ runner: { "keep": { "name": "shed runner", "group": "Mini", "running": true } } }),
    );
    assert_eq!(verdict(&rows(&stopped, &must), runner), down("not running"));
    assert_eq!(
        verdict(&rows(&fixture("launchd/list"), &must), runner),
        up("running")
    );
}

#[test]
fn kept_shows_its_name_unreviewed_has_no_group_ignored_and_removed_are_hidden_and_a_vanished_kept_job_is_down()
 {
    let r = reviews(json!({
        "dev.drover.tick": { "keep": { "name": "tick", "group": "Mini" } },
        "dev.gone": { "keep": { "name": "gone", "group": "Mini" } },
        "dev.old": "removed",
        "com.jirathip.caffeinate": "ignore",
        "com.jirathip.ttyd": "removed",
    }));
    let rows = rows(&fixture("launchd/list"), &r);
    let tick = &rows["dev.drover.tick"];
    assert_eq!(
        (tick.name.as_str(), tick.group.as_deref()),
        ("tick", Some("Mini"))
    );
    assert_eq!(tick.id, "job-dev-drover-tick");
    let gone = &rows["dev.gone"];
    assert_eq!(
        (gone.status, gone.message.as_str(), gone.group.as_deref()),
        (Status::Down, "not loaded", Some("Mini"))
    );
    assert!(
        !rows.contains_key("dev.old"),
        "a removed job that is gone is not shown"
    );
    assert!(
        !rows.contains_key("com.jirathip.caffeinate"),
        "an ignored job is not shown"
    );
    let back = &rows["com.jirathip.ttyd"];
    assert_eq!(
        (back.name.as_str(), back.group.as_deref()),
        ("com.jirathip.ttyd", None)
    );
    let boat = &rows["com.jirathip.boat-rt"];
    assert_eq!(
        (boat.id.as_str(), boat.name.as_str(), boat.group.as_deref()),
        ("job-com-jirathip-boat-rt", "com.jirathip.boat-rt", None)
    );
}

#[test]
fn ids_are_slugs_with_a_hash_when_long_or_shared_and_always_valid_ids() {
    let long = format!("dev.{}", "x".repeat(70));
    let text = format!("PID\tStatus\tLabel\n1\t0\ta.b\n1\t0\tA-b\n1\t0\t{long}\n1\t0\tdev.solo\n");
    let rows = rows(&text, &BTreeMap::new());
    let ids: Vec<_> = rows.values().map(|r| r.id.clone()).collect();
    assert_eq!(rows["dev.solo"].id, "job-dev-solo");
    assert!(rows["a.b"].id.starts_with("job-a-b-") && rows["a.b"].id.len() == 16);
    assert_ne!(rows["a.b"].id, rows["A-b"].id);
    assert_eq!(rows[long.as_str()].id.len(), 64);
    for id in ids {
        beacon::config::Id::try_from(id.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

fn units() -> Vec<String> {
    let files = std::fs::read_dir(format!("{FIX}/systemd/units")).unwrap();
    files
        .map(|f| f.unwrap().file_name().into_string().unwrap())
        .collect()
}

fn show(unit: &str) -> Option<Detail> {
    let read = |u: &str| std::fs::read_to_string(format!("{FIX}/systemd/show/{u}")).ok();
    let timer = if unit.strip_suffix(".timer").is_some() {
        Some(read(unit)?)
    } else {
        None
    };
    systemd::detail(&read(&systemd::service_of(unit))?, timer.as_deref())
}

#[test]
fn systemd_units_are_judged_by_restart_and_timer_with_a_timer_and_its_service_as_one_job() {
    let listing = systemd::listing(&fixture("systemd/list-units"), &units());
    let labels: Vec<_> = listing.iter().map(|j| j.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "agent.service",
            "boat-rt.timer",
            "once.service",
            "tick.timer",
            "worker.service"
        ]
    );
    assert_eq!(show("agent.service").unwrap().kind, Kind::KeepAlive);
    assert_eq!(show("boat-rt.timer").unwrap().kind, Kind::Scheduled(300));
    assert_eq!(show("tick.timer").unwrap().kind, Kind::Scheduled(86_400));
    assert_eq!(show("once.service").unwrap().kind, Kind::OnDemand);
    let mut w = Watcher::default();
    let rows = w.update(&listing, &BTreeMap::new(), Instant::now(), show);
    let r: BTreeMap<_, _> = rows.into_iter().map(|r| (r.label.clone(), r)).collect();
    assert_eq!(verdict(&r, "agent.service"), up("running"));
    assert_eq!(verdict(&r, "worker.service"), down("not running"));
    assert_eq!(verdict(&r, "once.service"), down("not running, failed"));
    assert_eq!(verdict(&r, "tick.timer"), down("failed"));
    assert_eq!(verdict(&r, "boat-rt.timer"), up("last exit 0"));
    let (t0, none) = (Instant::now(), BTreeMap::new());
    let mut w = Watcher::default();
    for s in [0, 300] {
        w.update(&listing, &none, t0 + Duration::from_secs(s), show);
    }
    let rows = w.update(&listing, &none, t0 + Duration::from_secs(600), show);
    let boat = rows.iter().find(|r| r.label == "boat-rt.timer").unwrap();
    assert_eq!(
        (boat.status, boat.message.as_str()),
        (Status::Down, "no run in 2 intervals of 300s")
    );
}

struct Serve(Child);

impl Drop for Serve {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("jobs-{name}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.path().is_dir() {
            copy(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn config(jobs: &Value) -> String {
    let checks = json!([{ "id": "ok", "command": "true", "every_s": 10, "timeout_s": 5 }]);
    let host = json!({ "url": "https://box/status", "checks": checks, "jobs": jobs });
    json!({ "version": 1, "monitors": [], "hosts": { "box": host } }).to_string()
}

fn serve(dir: &Path, tool: (&str, &str)) -> (Serve, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args([
            "serve",
            "--config",
            "monitors.json",
            "--host",
            "box",
            "--listen",
            "127.0.0.1:0",
        ])
        .current_dir(dir)
        .env("HOME", dir)
        .env("BEACON_SPEEDUP", "60")
        .env_remove("BEACON_LAUNCHCTL")
        .env(tool.0, format!("{FIX}/fake"))
        .env("FAKE_JOBS", dir.join(tool.1))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line
        .split(' ')
        .nth(3)
        .unwrap()
        .trim_end_matches(':')
        .to_string();
    (Serve(child), addr)
}

fn status(addr: &str) -> Value {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(
        s,
        "GET /status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn poll(addr: &str, done: impl Fn(&Value) -> bool) -> Value {
    let start = Instant::now();
    loop {
        let body = status(addr);
        if done(&body["checks"]) {
            return body["checks"].clone();
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "never got it: {body}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn serve_reports_launchd_jobs_in_status_with_name_and_group_and_follows_a_change() {
    let dir = scratch("serve-launchd");
    copy(Path::new(&format!("{FIX}/launchd")), &dir.join("launchd"));
    let jobs = json!({
        "dev.shed.serve": { "keep": { "name": "shed", "group": "Mini" } },
        "dev.gone": { "keep": { "name": "gone", "group": "Mini" } },
        "dev.old": "removed",
        "com.jirathip.caffeinate": "ignore",
    });
    std::fs::write(dir.join("monitors.json"), config(&jobs)).unwrap();
    let (_serve, addr) = serve(&dir, ("BEACON_LAUNCHCTL", "launchd"));
    let checks = poll(&addr, |c| {
        c.get("job-dev-shed-serve").is_some() && c.get("ok").is_some()
    });
    let shed = &checks["job-dev-shed-serve"];
    assert_eq!(
        (
            &shed["status"],
            &shed["message"],
            &shed["name"],
            &shed["group"]
        ),
        (
            &json!("up"),
            &json!("running"),
            &json!("shed"),
            &json!("Mini")
        )
    );
    let ttyd = &checks["job-com-jirathip-ttyd"];
    assert_eq!(
        (&ttyd["name"], ttyd.get("group")),
        (&json!("com.jirathip.ttyd"), None)
    );
    assert_eq!(checks["job-dev-gone"]["message"], "not loaded");
    assert_eq!(checks["job-dev-drover-clean"]["status"], "down");
    assert!(checks.get("job-com-jirathip-caffeinate").is_none());
    assert!(checks.get("job-dev-old").is_none());
    assert!(checks.get("job-dev-beacon-serve").is_none());
    assert!(checks.get("ok").unwrap().get("name").is_none());
    let list = dir.join("launchd/list");
    let text = std::fs::read_to_string(&list).unwrap();
    std::fs::write(
        &list,
        text.replace("50226\t-9\tdev.shed", "-\t-9\tdev.shed")
            .replace("1424\t1\tcom.jirathip.ttyd\n", ""),
    )
    .unwrap();
    let checks = poll(&addr, |c| c["job-dev-shed-serve"]["status"] == "down");
    assert_eq!(
        checks["job-dev-shed-serve"]["message"],
        "not running, last exit signal 9"
    );
    assert!(
        checks.get("job-com-jirathip-ttyd").is_none(),
        "a vanished unreviewed job leaves the list"
    );
}

#[test]
fn serve_reports_systemd_user_units_when_the_systemctl_variable_names_the_tool() {
    let dir = scratch("serve-systemd");
    copy(Path::new(&format!("{FIX}/systemd")), &dir.join("systemd"));
    copy(
        &dir.join("systemd/units"),
        &dir.join(".config/systemd/user"),
    );
    std::fs::write(dir.join("monitors.json"), config(&json!({}))).unwrap();
    let (_serve, addr) = serve(&dir, ("BEACON_SYSTEMCTL", "systemd"));
    let checks = poll(&addr, |c| c.get("job-agent-service").is_some());
    assert_eq!(checks["job-agent-service"]["message"], "running");
    assert_eq!(checks["job-tick-timer"]["status"], "down");
    assert!(checks.get("job-tick-service").is_none());
    assert!(checks.get("job-ghost-service").is_none());
    assert!(checks.get("job-beacon-serve-service").is_none());
}

fn jobs(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_beacon"))
        .arg("jobs")
        .args(args)
        .env_remove("BEACON_CONFIG")
        .output()
        .unwrap()
}

fn check(path: &Path) {
    let o = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .arg("check")
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
}

const EXAMPLE_BLOCK: &str = r#"{
        "com.example.caffeinate": "ignore",
        "dev.example.old": "removed",
        "dev.example.tick": {"keep":{"name":"tick","group":"macmini"}}
      }"#;

#[test]
fn keep_ignore_removed_rewrite_only_the_jobs_block_keep_the_file_valid_and_are_idempotent() {
    let dir = scratch("edit");
    let file = dir.join("monitors.json");
    let example = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/monitors.example.json"
    ))
    .unwrap();
    std::fs::write(&file, &example).unwrap();
    let path = file.to_str().unwrap();
    let o = jobs(&[
        "keep",
        "com.jirathip.ttyd",
        "--name",
        "ttyd",
        "--group",
        "Mini",
        "--config",
        path,
        "--host",
        "macmini",
    ]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.starts_with("~ macmini: keep com.jirathip.ttyd as \"ttyd\" in Mini\n"),
        "{out}"
    );
    assert!(out.contains(&format!("git -C {} commit -m \"beacon jobs: keep com.jirathip.ttyd\" monitors.json && git -C {} push", dir.display(), dir.display())), "{out}");
    let block = r#"{
        "com.example.caffeinate": "ignore",
        "com.jirathip.ttyd": {"keep":{"name":"ttyd","group":"Mini"}},
        "dev.example.old": "removed",
        "dev.example.tick": {"keep":{"name":"tick","group":"macmini"}}
      }"#;
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        example.replace(EXAMPLE_BLOCK, block)
    );
    check(&file);
    let o = jobs(&[
        "keep",
        "com.jirathip.ttyd",
        "--name",
        "ttyd",
        "--group",
        "Mini",
        "--config",
        path,
        "--host",
        "macmini",
    ]);
    assert!(
        String::from_utf8_lossy(&o.stdout).starts_with("= macmini: keep"),
        "{o:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        example.replace(EXAMPLE_BLOCK, block)
    );
    for (verb, review) in [("ignore", "\"ignore\""), ("removed", "\"removed\"")] {
        let o = jobs(&[
            verb,
            "com.jirathip.ttyd",
            "--config",
            path,
            "--host",
            "macmini",
        ]);
        assert_eq!(o.status.code(), Some(0), "{o:?}");
        let want = block.replace(r#"{"keep":{"name":"ttyd","group":"Mini"}}"#, review);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            example.replace(EXAMPLE_BLOCK, &want)
        );
        check(&file);
    }
}

#[test]
fn an_edit_adds_a_block_to_a_host_without_one_including_a_one_line_file() {
    let dir = scratch("edit-new");
    let file = dir.join("m.json");
    let path = file.to_str().unwrap();
    let pretty = "{\n  \"version\": 1,\n  \"monitors\": [],\n  \"hosts\": {\n    \"a\": {\n      \"url\": \"https://a/status\",\n      \"checks\": []\n    },\n    \"b\": { \"url\": \"https://b/status\", \"checks\": [] }\n  }\n}\n";
    std::fs::write(&file, pretty).unwrap();
    assert!(
        jobs(&["ignore", "x.y", "--config", path, "--host", "a"])
            .status
            .success()
    );
    let want = pretty.replace(
        "\"checks\": []\n    }",
        "\"checks\": [],\n      \"jobs\": {\n        \"x.y\": \"ignore\"\n      }\n    }",
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), want);
    check(&file);
    let line =
        r#"{"version":1,"monitors":[],"hosts":{"b":{"url":"https://b/status","checks":[]}}}"#;
    std::fs::write(&file, line).unwrap();
    assert!(
        jobs(&["removed", "x.y", "--config", path, "--host", "b"])
            .status
            .success()
    );
    let want = line.replace(
        r#""checks":[]}"#,
        "\"checks\":[],\n\"jobs\": {\n  \"x.y\": \"removed\"\n}}",
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), want);
    check(&file);
}

#[test]
fn jobs_refuses_a_bad_label_an_unknown_host_or_a_keep_without_name_with_exit_2_and_leaves_the_file()
{
    let dir = scratch("edit-bad");
    let file = dir.join("m.json");
    let path = file.to_str().unwrap();
    let text = config(&json!({}));
    std::fs::write(&file, &text).unwrap();
    for args in [
        vec!["ignore", "a b", "--config", path, "--host", "box"],
        vec!["ignore", "x", "--config", path, "--host", "nobox"],
        vec![
            "keep", "x", "--group", "g", "--config", path, "--host", "box",
        ],
        vec![
            "keep",
            "x",
            "--name",
            &"n".repeat(61),
            "--group",
            "g",
            "--config",
            path,
            "--host",
            "box",
        ],
    ] {
        let o = jobs(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {o:?}");
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn jobs_lists_each_job_with_its_review_and_status() {
    let dir = scratch("list");
    copy(Path::new(&format!("{FIX}/launchd")), &dir.join("launchd"));
    let jobs_block = json!({
        "dev.shed.serve": { "keep": { "name": "shed", "group": "Mini" } },
        "dev.gone": { "keep": { "name": "gone", "group": "Mini" } },
        "dev.old": "removed",
        "com.jirathip.caffeinate": "ignore",
    });
    std::fs::write(dir.join("m.json"), config(&jobs_block)).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args(["jobs", "--config", "m.json", "--host", "box"])
        .current_dir(&dir)
        .env("BEACON_LAUNCHCTL", format!("{FIX}/fake"))
        .env("FAKE_JOBS", dir.join("launchd"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let out = String::from_utf8_lossy(&o.stdout);
    let line = |label: &str| {
        let l = out
            .lines()
            .find(|l| l.split_whitespace().nth(2) == Some(label));
        l.map_or_default(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    assert_eq!(
        line("dev.shed.serve"),
        "kept up dev.shed.serve running \"shed\" in Mini"
    );
    assert_eq!(
        line("dev.gone"),
        "kept down dev.gone not loaded \"gone\" in Mini"
    );
    assert_eq!(
        line("com.jirathip.caffeinate"),
        "ignored down com.jirathip.caffeinate not running"
    );
    assert_eq!(line("dev.old"), "removed - dev.old not loaded");
    assert_eq!(
        line("dev.drover.clean"),
        "unreviewed down dev.drover.clean last exit 1"
    );
}
