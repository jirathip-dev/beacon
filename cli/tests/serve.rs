use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Serve(Child);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start() -> (Serve, String) {
    let path = format!("{}/serve.json", env!("CARGO_TARGET_TMPDIR"));
    let checks = json!([{ "id": "ok", "command": "echo fine", "every_s": 10, "timeout_s": 5 }]);
    let config = json!({ "version": 1, "monitors": [], "hosts": { "box": { "url": "https://box/status", "checks": checks } } });
    std::fs::write(&path, config.to_string()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args([
            "serve",
            "--config",
            &path,
            "--host",
            "box",
            "--listen",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("serve box on 127.0.0.1:"), "{line}");
    assert!(
        line.contains("1 checks (ok every 10s), metrics every 60s (/)"),
        "{line}"
    );
    let addr = line
        .split(' ')
        .nth(3)
        .unwrap()
        .trim_end_matches(':')
        .to_string();
    (Serve(child), addr)
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
        std::thread::sleep(Duration::from_millis(100));
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
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("listen"));
}
