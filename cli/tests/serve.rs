use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Serve(Child);

impl Drop for Serve {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

fn config(id: &str) -> String {
    let checks = json!([{ "id": id, "command": "echo fine", "every_s": 10, "timeout_s": 5 }]);
    json!({ "version": 1, "monitors": [], "hosts": { "box": { "url": "https://box/status", "checks": checks } } })
        .to_string()
}

fn start() -> (Serve, String) {
    let path = format!("{}/serve.json", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, config("ok")).unwrap();
    let (serve, addr, line) = start_at(&path);
    assert!(
        line.contains("1 checks (ok every 10s), metrics every 60s (/)"),
        "{line}"
    );
    (serve, addr)
}

fn start_at(path: &str) -> (Serve, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args([
            "serve",
            "--config",
            path,
            "--host",
            "box",
            "--listen",
            "127.0.0.1:0",
        ])
        .env("HOME", env!("CARGO_TARGET_TMPDIR"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("serve box on 127.0.0.1:"), "{line}");
    let addr = line
        .split(' ')
        .nth(3)
        .unwrap()
        .trim_end_matches(':')
        .to_string();
    (Serve(child), addr, line)
}

fn get(addr: &str, path: &str, etag: Option<&str>) -> (u16, Option<String>, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let inm = etag.map_or(String::new(), |e| format!("If-None-Match: {e}\r\n"));
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: x\r\n{inm}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let code = head[9..12].parse().unwrap();
    let etag = head
        .lines()
        .find_map(|l| l.strip_prefix("ETag: "))
        .map(String::from);
    (code, etag, body.into())
}

fn poll(addr: &str, path: &str, done: impl Fn(&Value) -> bool) -> (String, Value) {
    for _ in 0..100 {
        let (code, etag, body) = get(addr, path, None);
        assert_eq!(code, 200, "{path}: {body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        if done(&v) {
            return (etag.expect("an ETag"), v);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("{path} never got there");
}

#[test]
fn serve_answers_metrics_and_status_with_etags_304s_and_404() {
    let (_serve, addr) = start();
    let (etag, m) = poll(&addr, "/metrics", |v| {
        v["t"].as_array().is_some_and(|t| t.len() == 1)
    });
    assert_eq!(
        (m["host"].clone(), m["every_s"].clone()),
        (json!("box"), json!(60))
    );
    assert!(
        m["load1"][0].is_number() && m["mem"]["used_bytes"][0].is_number(),
        "{m}"
    );
    assert!(m["disks"]["/"]["free_bytes"][0].is_number(), "{m}");
    assert_eq!(get(&addr, "/metrics", Some(&etag)).0, 304);
    let (etag, s) = poll(&addr, "/status", |v| v["checks"]["ok"].is_object());
    assert_eq!(
        (
            s["checks"]["ok"]["status"].clone(),
            s["checks"]["ok"]["message"].clone()
        ),
        (json!("up"), json!("fine"))
    );
    assert!(
        s["checks"]["ok"]["since"].as_str().unwrap().ends_with('Z'),
        "{s}"
    );
    assert_eq!(get(&addr, "/status", Some(&etag)).0, 304);
    assert_eq!(get(&addr, "/x", None).0, 404);
}

#[test]
fn serve_exits_2_when_it_cannot_listen() {
    let (_serve, addr) = start();
    let path = format!("{}/serve.json", env!("CARGO_TARGET_TMPDIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args([
            "serve", "--config", &path, "--host", "box", "--listen", &addr,
        ])
        .env("HOME", env!("CARGO_TARGET_TMPDIR"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("listen"));
}

#[test]
fn serve_answers_config_with_etag_and_reloads_only_a_valid_change() {
    let path = format!("{}/serve-config.json", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, config("ok")).unwrap();
    let (_serve, addr, _) = start_at(&path);
    let (code, etag, body) = get(&addr, "/config", None);
    assert_eq!((code, body), (200, config("ok")));
    assert_eq!(get(&addr, "/config", etag.as_deref()).0, 304);
    poll(&addr, "/status", |v| v["checks"]["ok"].is_object());
    std::fs::write(&path, config("new")).unwrap();
    poll(&addr, "/config", |v| {
        v["hosts"]["box"]["checks"][0]["id"] == "new"
    });
    let (_, s) = poll(&addr, "/status", |v| v["checks"]["new"].is_object());
    assert!(s["checks"]["ok"].is_null(), "{s}");
    assert_ne!(get(&addr, "/config", etag.as_deref()).0, 304);
    std::fs::write(&path, "{ not json").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let (code, _, body) = get(&addr, "/config", None);
    assert_eq!((code, body), (200, config("new")));
}

#[test]
fn serve_pulls_a_git_checkout_of_its_config() {
    let root = format!("{}/serve-git", env!("CARGO_TARGET_TMPDIR"));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let git = |dir: &str, args: &[&str]| {
        let ok = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C", dir])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    let (origin, up, clone) = (
        format!("{root}/origin.git"),
        format!("{root}/up"),
        format!("{root}/clone"),
    );
    git(&root, &["init", "-q", "--bare", &origin]);
    git(&root, &["clone", "-q", &origin, &up]);
    std::fs::write(format!("{up}/monitors.json"), config("ok")).unwrap();
    git(&up, &["add", "."]);
    git(&up, &["commit", "-qm", "one"]);
    git(&up, &["push", "-q", "origin", "HEAD"]);
    git(&root, &["clone", "-q", &origin, &clone]);
    std::fs::write(format!("{up}/monitors.json"), config("pulled")).unwrap();
    git(&up, &["commit", "-qam", "two"]);
    git(&up, &["push", "-q", "origin", "HEAD"]);
    let (_serve, addr, _) = start_at(&format!("{clone}/monitors.json"));
    poll(&addr, "/status", |v| v["checks"]["pulled"].is_object());
}
