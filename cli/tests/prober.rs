use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const TLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls");

struct Fake {
    port: u16,
    hooks: Arc<Mutex<Vec<Value>>>,
}

impl Fake {
    fn start(cert: &str, key: &str) -> Fake {
        let certs = CertificateDer::pem_file_iter(cert)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let key = PrivateKeyDer::from_pem_file(key).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
        let (tls, listener) = (Arc::new(tls), TcpListener::bind("127.0.0.1:0").unwrap());
        let port = listener.local_addr().unwrap().port();
        let hooks = Arc::new(Mutex::new(Vec::new()));
        let seen = hooks.clone();
        thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let (tls, seen) = (tls.clone(), seen.clone());
                thread::spawn(move || answer(tcp, tls, &seen));
            }
        });
        Fake { port, hooks }
    }

    fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }
}

fn answer(tcp: TcpStream, tls: Arc<ServerConfig>, hooks: &Mutex<Vec<Value>>) {
    static FLIPS: AtomicUsize = AtomicUsize::new(0);
    let mut s = StreamOwned::new(ServerConnection::new(tls).unwrap(), tcp);
    let mut r = BufReader::new(&mut s);
    let (mut head, mut line) = (String::new(), String::new());
    while line != "\r\n" {
        line.clear();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        head.push_str(&line.to_ascii_lowercase());
    }
    if head.starts_with("get /flip") && FLIPS.fetch_add(1, Ordering::Relaxed) >= 2 {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    if head.starts_with("post /hook") {
        let len = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .map_or(0, |n| n.trim().parse().unwrap());
        let mut body = vec![0; len];
        r.read_exact(&mut body).unwrap();
        hooks
            .lock()
            .unwrap()
            .push(serde_json::from_slice(&body).unwrap());
    }
    let body = "hello beacon";
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(reply.as_bytes()).ok();
    s.conn.send_close_notify();
    s.flush().ok();
}

struct Serve {
    child: Child,
    addr: String,
}

impl Drop for Serve {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("prober-{name}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn serve(dir: &Path, host: &str, ca: &str, notify: bool) -> Serve {
    let mut c = Command::new(env!("CARGO_BIN_EXE_beacon"));
    c.args(["serve", "--config", "monitors.json", "--host", host])
        .args(["--listen", "127.0.0.1:0"])
        .current_dir(dir)
        .env("HOME", dir)
        .env("BEACON_SPEEDUP", "60")
        .env("SSL_CERT_FILE", ca)
        .env("BEACON_LAUNCHCTL", "true")
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(dir.join(format!("{host}.stderr"))).unwrap());
    if notify {
        c.args(["--notify", "notify.json"]);
    }
    let mut child = c.spawn().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let addr = line.split(' ').nth(3).unwrap().trim_end_matches(':').into();
    thread::spawn(move || std::io::copy(&mut out, &mut std::io::sink()));
    Serve { child, addr }
}

fn get(addr: &str, path: &str, etag: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: x\r\nIf-None-Match: {etag}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let tag = head.lines().find_map(|l| l.strip_prefix("ETag: "));
    let code = head[9..12].parse().unwrap();
    (code, tag.unwrap_or_default().into(), body.into())
}

fn poll(addr: &str, path: &str, done: impl Fn(&Value) -> bool) -> (Value, String) {
    let start = Instant::now();
    loop {
        let (code, etag, body) = get(addr, path, "");
        if code == 200 {
            let v: Value = serde_json::from_str(&body).unwrap();
            if done(&v) {
                return (v, etag);
            }
            assert!(start.elapsed() < Duration::from_secs(20), "{path}: {v}");
        }
        assert!(start.elapsed() < Duration::from_secs(20), "{path}: {code}");
        thread::sleep(Duration::from_millis(100));
    }
}

fn monitor<'v>(history: &'v Value, name: &str) -> &'v Value {
    let monitors = history["monitors"].as_array().unwrap();
    monitors.iter().find(|m| m["name"] == name).unwrap()
}

fn http(name: &str, url: &str, extra: &Value) -> Value {
    let mut m = json!({ "type": "http", "name": name, "group": "Web", "url": url, "timeout_s": 1 });
    m.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    m
}

fn config(monitors: &[Value], checks: &[Value]) -> String {
    let hosts = json!({
        "box": { "url": "https://box/status", "checks": checks },
        "other": { "url": "https://other/status", "checks": [] },
    });
    json!({ "version": 1, "prober": "box", "monitors": monitors, "hosts": hosts }).to_string()
}

#[test]
fn the_prober_keeps_times_and_incidents_and_notifies_and_other_hosts_probe_nothing() {
    let fake = Fake::start(&format!("{TLS}/cert.pem"), &format!("{TLS}/key.pem"));
    let dir = scratch("probe");
    let monitors = [
        http("Ok", &fake.url("/ok"), &json!({ "keyword": "beacon" })),
        http("Hang", &fake.url("/flip"), &json!({})),
        json!({ "type": "tcp", "name": "Port", "group": "Hosts", "host": "127.0.0.1", "port": fake.port, "timeout_s": 1 }),
    ];
    std::fs::write(dir.join("monitors.json"), config(&monitors, &[])).unwrap();
    let hook = json!({ "url": fake.url("/hook"), "secret": "s" }).to_string();
    std::fs::write(dir.join("notify.json"), hook).unwrap();
    let mode = std::os::unix::fs::PermissionsExt::from_mode(0o600);
    std::fs::set_permissions(dir.join("notify.json"), mode).unwrap();
    let ca = format!("{TLS}/ca.pem");
    let prober = serve(&dir, "box", &ca, true);
    let (h, _) = poll(&prober.addr, "/history", |h| {
        monitor(h, "Hang")["down"][0]["message"] == "timeout after 1s"
            && monitor(h, "Hang")["down"][0]["down_at"].as_u64()
                > Some(monitor(h, "Hang")["since"].as_u64().unwrap())
    });
    assert_eq!(monitor(&h, "Ok")["down"], json!([]), "{h}");
    assert_eq!(monitor(&h, "Port")["down"], json!([]), "{h}");
    let times = |name: &str| {
        let id = monitor(&h, name)["id"].as_str().unwrap().to_string();
        let (t, etag) = poll(&prober.addr, &format!("/history/{id}"), |t| {
            !t["recent"]["ms"].as_array().unwrap().is_empty()
        });
        assert_eq!(get(&prober.addr, &format!("/history/{id}"), &etag).0, 304);
        (id, t["recent"]["ms"][0].clone())
    };
    let (ok_id, ok_ms) = times("Ok");
    assert!(ok_ms.is_u64(), "{ok_ms}");
    assert!(times("Port").1.is_u64());
    let hang = times("Hang").0;
    let (t, _) = poll(&prober.addr, &format!("/history/{hang}"), |t| {
        t["recent"]["ms"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .is_null()
    });
    assert!(
        t["recent"]["t"].as_array().unwrap().len() <= 2,
        "a minute, a slot: {t}"
    );
    let ring = dir.join(format!(".local/state/beacon/history/times/{ok_id}.bin"));
    assert_eq!(std::fs::metadata(ring).unwrap().len(), 5776);
    let start = Instant::now();
    while !fake
        .hooks
        .lock()
        .unwrap()
        .iter()
        .any(|e| e["check"] == "Web/Hang")
    {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no Web/Hang post"
        );
        thread::sleep(Duration::from_millis(100));
    }
    let hooks = fake.hooks.lock().unwrap().clone();
    let hang = hooks.iter().find(|e| e["check"] == "Web/Hang").unwrap();
    assert_eq!(
        (&hang["status"], &hang["message"]),
        (&json!("down"), &json!("timeout after 1s"))
    );

    let other = serve(&dir, "other", &ca, false);
    let (h, _) = poll(&other.addr, "/history", |_| true);
    assert_eq!((&h["host"], &h["monitors"]), (&json!("other"), &json!([])));
    assert_eq!(get(&other.addr, &format!("/history/{ok_id}"), "").0, 404);
}

#[test]
fn a_certificate_expiring_in_10_days_is_down_unless_cert_warn_days_is_shorter() {
    let dir = scratch("cert");
    let d = dir.display();
    let ext = dir.join("ext");
    std::fs::write(&ext, "subjectAltName=IP:127.0.0.1\n").unwrap();
    let ec = [
        "-newkey",
        "ec",
        "-pkeyopt",
        "ec_paramgen_curve:prime256v1",
        "-nodes",
    ];
    let openssl = |args: &[&str]| {
        let ok = Command::new("openssl")
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "openssl {args:?}: {ok:?}");
    };
    openssl(
        &[
            &["req", "-x509"][..],
            &ec,
            &[
                "-keyout", "ca.key", "-out", "ca.pem", "-days", "30", "-subj", "/CN=t",
            ],
        ]
        .concat(),
    );
    openssl(
        &[
            &["req"][..],
            &ec,
            &[
                "-keyout",
                "key.pem",
                "-out",
                "leaf.csr",
                "-subj",
                "/CN=127.0.0.1",
            ],
        ]
        .concat(),
    );
    openssl(&[
        "x509",
        "-req",
        "-in",
        "leaf.csr",
        "-CA",
        "ca.pem",
        "-CAkey",
        "ca.key",
        "-CAcreateserial",
        "-days",
        "10",
        "-extfile",
        "ext",
        "-out",
        "cert.pem",
    ]);
    let fake = Fake::start(&format!("{d}/cert.pem"), &format!("{d}/key.pem"));
    let monitors = [
        http("Soon", &fake.url("/"), &json!({})),
        http("Late", &fake.url("/"), &json!({ "cert_warn_days": 5 })),
    ];
    std::fs::write(dir.join("monitors.json"), config(&monitors, &[])).unwrap();
    let prober = serve(&dir, "box", &format!("{d}/ca.pem"), false);
    let (h, _) = poll(&prober.addr, "/history", |h| {
        !monitor(h, "Soon")["down"].as_array().unwrap().is_empty()
    });
    let message = monitor(&h, "Soon")["down"][0]["message"].clone();
    assert!(
        message == "cert expires in 9d" || message == "cert expires in 10d",
        "{message}"
    );
    assert_eq!(monitor(&h, "Late")["down"], json!([]), "{h}");
}

#[test]
fn an_open_incident_survives_a_restart_with_the_stop_as_a_gap_and_a_corrupt_store_starts_empty() {
    let dir = scratch("restart");
    let check =
        json!({ "id": "bad", "command": "echo broken; false", "every_s": 10, "timeout_s": 5 });
    std::fs::write(dir.join("monitors.json"), config(&[], &[check])).unwrap();
    let events = dir.join(".local/state/beacon/history/events.json");
    let first = serve(&dir, "box", "", false);
    let (h, _) = poll(&first.addr, "/history", |h| {
        h["checks"]["bad"]["down"][0].is_object()
    });
    let down_at = h["checks"]["bad"]["down"][0]["down_at"].clone();
    let start = Instant::now();
    while !std::fs::read(&events).is_ok_and(|b| b.windows(6).any(|w| w == b"broken")) {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "events.json never written"
        );
        thread::sleep(Duration::from_millis(100));
    }
    drop(first);
    let stopped = SystemTime::now() - Duration::from_secs(600);
    std::fs::File::options()
        .write(true)
        .open(&events)
        .unwrap()
        .set_modified(stopped)
        .unwrap();
    let again = serve(&dir, "box", "", false);
    let (h, _) = poll(&again.addr, "/history", |h| {
        !h["gaps"].as_array().unwrap().is_empty()
    });
    let down = &h["checks"]["bad"]["down"];
    assert_eq!(
        (
            down.as_array().unwrap().len(),
            &down[0]["down_at"],
            &down[0]["up_at"]
        ),
        (1, &down_at, &Value::Null),
        "{h}"
    );
    let gap = &h["gaps"][0];
    assert!(
        gap[1].as_u64().unwrap() - gap[0].as_u64().unwrap() >= 600,
        "{h}"
    );
    drop(again);
    std::fs::write(&events, "{ not json").unwrap();
    let corrupt = serve(&dir, "box", "", false);
    let (h, _) = poll(&corrupt.addr, "/history", |_| true);
    assert_eq!(
        (&h["gaps"], &h["monitors"]),
        (&json!([]), &json!([])),
        "{h}"
    );
    assert_ne!(h["checks"]["bad"]["down"][0]["down_at"], down_at, "{h}");
    let log = std::fs::read_to_string(dir.join("box.stderr")).unwrap();
    assert!(log.contains("events.json: unreadable"), "{log}");
}
