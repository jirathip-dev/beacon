use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const TLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls");
const SECRET: &str = "s3cret-never-logged";

enum Reply {
    Answer(u16, String, String),
    Drop,
    Hang,
}

#[derive(Clone)]
struct Got {
    head: String,
    body: Vec<u8>,
    at: Instant,
}

struct Fake {
    port: u16,
    got: Arc<Mutex<Vec<Got>>>,
}

impl Fake {
    fn start(answer: impl Fn(usize, &str) -> Reply + Send + Sync + 'static) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let got = Arc::new(Mutex::new(Vec::new()));
        let (seen, answer, tls) = (got.clone(), Arc::new(answer), tls());
        thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let (seen, answer, tls) = (seen.clone(), answer.clone(), tls.clone());
                thread::spawn(move || one(tcp, &tls, &seen, &*answer));
            }
        });
        Fake { port, got }
    }

    fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }

    fn got(&self) -> Vec<Got> {
        self.got.lock().unwrap().clone()
    }

    fn posts(&self) -> Vec<Value> {
        let got = self.got();
        got.iter()
            .map(|g| serde_json::from_slice(&g.body).unwrap())
            .collect()
    }
}

fn tls() -> Arc<ServerConfig> {
    let certs = CertificateDer::pem_file_iter(format!("{TLS}/cert.pem"))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let key = PrivateKeyDer::from_pem_file(format!("{TLS}/key.pem")).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    Arc::new(config)
}

fn one(
    tcp: TcpStream,
    tls: &Arc<ServerConfig>,
    seen: &Mutex<Vec<Got>>,
    answer: &dyn Fn(usize, &str) -> Reply,
) {
    let mut s = StreamOwned::new(ServerConnection::new(tls.clone()).unwrap(), tcp);
    let mut r = BufReader::new(&mut s);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
        head.push_str(&line.to_ascii_lowercase());
    }
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length: "))
        .map_or(0, |n| n.trim().parse().unwrap());
    let mut body = vec![0; len];
    r.read_exact(&mut body).unwrap();
    let n = {
        let mut seen = seen.lock().unwrap();
        seen.push(Got {
            head: head.clone(),
            body,
            at: Instant::now(),
        });
        seen.len() - 1
    };
    match answer(n, &head) {
        Reply::Answer(code, headers, body) => {
            let reply = format!(
                "HTTP/1.1 {code} X\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).ok();
            s.conn.send_close_notify();
            s.flush().ok();
        }
        Reply::Drop => {}
        Reply::Hang => thread::sleep(Duration::from_secs(120)),
    }
}

fn status_at(checks: &[(&str, &str)], n: usize) -> Reply {
    let at = |seq: &str| seq.chars().nth(n).or(seq.chars().last()).unwrap();
    let mut body = json!({ "version": 1, "host": "r", "checks": {} });
    let mut tag = String::new();
    for (id, seq) in checks {
        let status = if at(seq) == 'u' { "up" } else { "down" };
        body["checks"][id] = json!({ "status": status, "message": format!("{id} {status}"), "since": "2026-11-20T10:15:00Z" });
        tag.push(at(seq));
    }
    Reply::Answer(200, format!("ETag: \"{tag}\"\r\n"), body.to_string())
}

fn host(checks: &'static [(&'static str, &'static str)]) -> impl Fn(usize, &str) -> Reply {
    move |n, head| match status_at(checks, n) {
        Reply::Answer(_, etag, _)
            if head.contains(&etag.to_ascii_lowercase().replace("etag", "if-none-match")) =>
        {
            Reply::Answer(304, etag, String::new())
        }
        r => r,
    }
}

fn receiver(code: u16) -> Fake {
    Fake::start(move |_, _| Reply::Answer(code, String::new(), String::new()))
}

struct Serve {
    child: Child,
    dir: PathBuf,
    addr: String,
}

impl Drop for Serve {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

impl Serve {
    fn start(name: &str, config: &str, hook: &Fake) -> Serve {
        Serve::start_alive(name, config, hook, 0)
    }

    fn start_alive(name: &str, config: &str, hook: &Fake, alive_every_s: u64) -> Serve {
        let dir = scratch(name);
        std::fs::write(dir.join("monitors.json"), config).unwrap();
        let notify = dir.join("notify.json");
        let file = json!({ "url": hook.url("/hooks/in/abc"), "secret": SECRET, "alive_every_s": alive_every_s });
        std::fs::write(&notify, file.to_string()).unwrap();
        std::fs::set_permissions(&notify, PermissionsExt::from_mode(0o600)).unwrap();
        let mut child = beacon(&dir)
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(dir.join("stderr")).unwrap())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        assert!(line.starts_with("serve box on"), "{line}");
        let addr = line["serve box on ".len()..]
            .split(": ")
            .next()
            .unwrap()
            .into();
        thread::spawn(move || std::io::copy(&mut out, &mut std::io::sink()));
        Serve { child, dir, addr }
    }

    fn status_ms(&self) -> u128 {
        let started = Instant::now();
        let mut tcp = TcpStream::connect(&self.addr).unwrap();
        write!(
            tcp,
            "GET /status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut reply = String::new();
        tcp.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        started.elapsed().as_millis()
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(self.dir.join("stderr")).unwrap()
    }

    fn reload(&self, config: &str) {
        std::fs::write(self.dir.join("monitors.json"), config).unwrap();
        wait("the reload", || self.stderr().contains("reload "));
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("notify-{name}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn beacon(dir: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_beacon"));
    c.args(["serve", "--config", "monitors.json", "--host", "box"])
        .args(["--listen", "127.0.0.1:0", "--notify", "notify.json"])
        .current_dir(dir)
        .env("HOME", dir)
        .env("BEACON_SPEEDUP", "60")
        .env("SSL_CERT_FILE", format!("{TLS}/ca.pem"));
    c
}

fn wait(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "never got {what}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn own(dir_name: &str, id: &str, seq: &str) -> Value {
    let f = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("notify-{dir_name}-{id}.n"));
    std::fs::remove_file(&f).ok();
    let f = f.display();
    let last = seq.chars().last().unwrap();
    let command = format!(
        "n=$(cat {f} 2>/dev/null || echo 0); echo $((n+1)) > {f}; c=$(printf %s {seq} | cut -c$((n+1))); c=${{c:-{last}}}; echo \"{id} $c\"; [ $c = u ]"
    );
    json!({ "id": id, "command": command, "every_s": 10, "timeout_s": 5 })
}

fn config(checks: &[Value], remotes: &[(&str, &str, u8)]) -> String {
    let mut hosts = json!({ "box": { "url": "https://box/status", "checks": checks } });
    for (name, url, timeout_s) in remotes {
        hosts[name] = json!({ "url": url, "timeout_s": timeout_s, "checks": [] });
    }
    json!({ "version": 1, "monitors": [], "hosts": hosts }).to_string()
}

fn settle() {
    thread::sleep(Duration::from_secs(4));
}

#[test]
fn a_change_held_for_2_polls_sends_one_signed_post_and_a_blip_sends_none() {
    let hook = receiver(200);
    let r1 = Fake::start(host(&[("disk", "uudd"), ("blip", "uudu")]));
    let checks = [own("change", "fall", "uudd"), own("change", "blip", "uudu")];
    let serve = Serve::start(
        "change",
        &config(&checks, &[("r1", &r1.url("/status"), 5)]),
        &hook,
    );
    wait("two posts", || hook.got().len() >= 2);
    settle();
    let mut posts = hook.posts();
    posts.sort_by_key(|p| p["host"].to_string());
    let keys: Vec<_> = posts
        .iter()
        .map(|p| {
            (
                p["kind"].clone(),
                p["host"].clone(),
                p["check"].clone(),
                p["status"].clone(),
            )
        })
        .collect();
    assert_eq!(
        keys,
        [
            (json!("check"), json!("box"), json!("fall"), json!("down")),
            (json!("check"), json!("r1"), json!("disk"), json!("down")),
        ]
    );
    assert_eq!(posts[0]["message"], "fall d");
    assert_eq!(posts[1]["message"], "disk down");
    let at = posts[0]["at"].as_str().unwrap();
    assert!(
        at.len() == 20 && at.ends_with('Z') && at.as_bytes()[10] == b'T',
        "{at}"
    );
    for g in hook.got() {
        assert!(g.head.starts_with("post /hooks/in/abc "), "{}", g.head);
        assert!(
            g.head.contains("content-type: application/json\r\n"),
            "{}",
            g.head
        );
        let want = format!(
            "x-signature-256: {}\r\n",
            beacon::notify::sign(SECRET.as_bytes(), &g.body)
        );
        assert!(g.head.contains(&want), "{}", g.head);
    }
    assert!(
        r1.got()
            .iter()
            .any(|g| g.head.contains("if-none-match: \"uu\""))
    );
    drop(serve);
}

fn alive(hook: &Fake) -> Vec<Got> {
    let got = hook.got().into_iter();
    got.filter(|g| serde_json::from_slice::<Value>(&g.body).unwrap()["kind"] == "alive")
        .collect()
}

#[test]
fn alive_is_sent_signed_every_interval_with_the_host_count() {
    let hook = receiver(200);
    let r1 = Fake::start(|_, _| Reply::Drop);
    let r2 = Fake::start(host(&[("disk", "u")]));
    let remotes = [("r1", r1.url("/status"), 5), ("r2", r2.url("/status"), 5)];
    let remotes: Vec<_> = remotes
        .iter()
        .map(|(n, u, t)| (*n, u.as_str(), *t))
        .collect();
    let started = Instant::now();
    let _serve = Serve::start_alive("alive", &config(&[], &remotes), &hook, 60);
    wait("6 alive posts", || alive(&hook).len() >= 6);
    let got = alive(&hook);
    let first = got[0].at.duration_since(started);
    assert!(
        (Duration::from_millis(700)..Duration::from_millis(1800)).contains(&first),
        "{first:?}"
    );
    for pair in got.windows(2) {
        let gap = pair[1].at.duration_since(pair[0].at);
        assert!(
            (Duration::from_millis(700)..Duration::from_millis(1500)).contains(&gap),
            "{gap:?}"
        );
    }
    for g in &got {
        assert!(g.head.starts_with("post /hooks/in/abc "), "{}", g.head);
        let want = format!(
            "x-signature-256: {}\r\n",
            beacon::notify::sign(SECRET.as_bytes(), &g.body)
        );
        assert!(g.head.contains(&want), "{}", g.head);
        let body: Value = serde_json::from_slice(&g.body).unwrap();
        let keys: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["at", "check", "host", "kind", "message", "status"]);
        assert_eq!(
            (&body["host"], &body["check"], &body["status"]),
            (&json!("box"), &Value::Null, &json!("up"))
        );
        let at = body["at"].as_str().unwrap();
        assert!(at.len() == 20 && at.ends_with('Z'), "{at}");
    }
    let message = |g: &Got| serde_json::from_slice::<Value>(&g.body).unwrap()["message"].clone();
    assert_eq!(message(&got[0]), "3 hosts, 0 down");
    assert_eq!(message(&got[5]), "3 hosts, 1 down");
}

#[test]
fn alive_every_s_0_sends_none() {
    let hook = receiver(200);
    let _serve = Serve::start_alive("alive-off", &config(&[], &[]), &hook, 0);
    settle();
    assert_eq!(hook.got().len(), 0);
}

#[test]
fn a_hung_alive_receiver_does_not_slow_status_or_change_messages() {
    let hook = Fake::start(|n, _| {
        if n == 0 {
            Reply::Hang
        } else {
            Reply::Answer(200, String::new(), String::new())
        }
    });
    let serve = Serve::start_alive(
        "alive-hung",
        &config(&[own("alive-hung", "fall", "uuuuuuuuuuuuuuuuudd")], &[]),
        &hook,
        60,
    );
    wait("the hung alive", || alive(&hook).len() == 1);
    let hung_at = Instant::now();
    for _ in 0..10 {
        assert!(serve.status_ms() < 500);
    }
    wait("the change", || hook.got().len() > alive(&hook).len());
    assert!(
        hung_at.elapsed() < Duration::from_secs(5),
        "{:?}",
        hung_at.elapsed()
    );
}

#[test]
fn a_failing_alive_receiver_is_logged_once_per_outage_and_never_retried() {
    let hook = receiver(500);
    let serve = Serve::start_alive("alive-fail", &config(&[], &[]), &hook, 60);
    wait("4 alive posts", || alive(&hook).len() >= 4);
    let got = alive(&hook);
    let bodies: std::collections::BTreeSet<_> = got.iter().map(|g| g.body.clone()).collect();
    assert_eq!(bodies.len(), got.len());
    let err = serve.stderr();
    assert_eq!(
        err.matches("notify alive failed: HTTP 500").count(),
        1,
        "{err}"
    );
    assert!(!err.contains(SECRET), "{err}");
}

#[test]
fn signature_matches_rfc_4231_test_vector_2() {
    assert_eq!(
        beacon::notify::sign(b"Jefe", b"what do ya want for nothing?"),
        "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn a_host_that_stops_answering_sends_down_after_3_failed_polls_and_up_when_it_answers() {
    let hook = receiver(200);
    let ok = host(&[("disk", "u")]);
    let r1 = Fake::start(move |n, head| {
        if (2..=5).contains(&n) {
            Reply::Drop
        } else {
            ok(n, head)
        }
    });
    let _serve = Serve::start(
        "host",
        &config(&[], &[("r1", &r1.url("/status"), 5)]),
        &hook,
    );
    wait("host down", || !hook.got().is_empty());
    let polls_at_down = r1.got().len();
    wait("host up", || hook.got().len() >= 2);
    settle();
    let posts = hook.posts();
    assert_eq!(posts.len(), 2, "{posts:?}");
    assert_eq!(
        (
            &posts[0]["kind"],
            &posts[0]["host"],
            &posts[0]["check"],
            &posts[0]["status"]
        ),
        (&json!("host"), &json!("r1"), &Value::Null, &json!("down"))
    );
    assert!(
        posts[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("unreachable: "),
        "{}",
        posts[0]
    );
    assert_eq!(
        (&posts[1]["kind"], &posts[1]["status"]),
        (&json!("host"), &json!("up"))
    );
    assert!((5..=6).contains(&polls_at_down), "{polls_at_down}");
}

#[test]
fn nothing_is_sent_on_start_up_or_on_a_config_reload() {
    let hook = receiver(200);
    let flip = Arc::new(AtomicBool::new(false));
    let f = flip.clone();
    let r1 = Fake::start(move |n, head| {
        let seq: &'static [(&str, &str)] = if f.load(Ordering::SeqCst) {
            &[("disk", "u")]
        } else {
            &[("disk", "d")]
        };
        host(seq)(n, head)
    });
    let r2 = Fake::start(host(&[("disk", "d")]));
    let gone = TcpListener::bind("127.0.0.1:0").unwrap();
    let gone_url = format!(
        "https://127.0.0.1:{}/status",
        gone.local_addr().unwrap().port()
    );
    drop(gone);
    let remotes = [("r1", r1.url("/status")), ("gone", gone_url)];
    let remotes: Vec<_> = remotes.iter().map(|(n, u)| (*n, u.as_str(), 5)).collect();
    let serve = Serve::start("quiet", &config(&[own("quiet", "a", "d")], &remotes), &hook);
    settle();
    assert_eq!(hook.got().len(), 0);
    let mut more = remotes.clone();
    let r2_url = r2.url("/status");
    more.push(("r2", &r2_url, 5));
    serve.reload(&config(
        &[own("quiet", "a", "d"), own("quiet", "b", "d")],
        &more,
    ));
    settle();
    assert!(!r2.got().is_empty());
    assert_eq!(hook.got().len(), 0);
    flip.store(true, Ordering::SeqCst);
    wait("the flip", || !hook.got().is_empty());
    settle();
    let posts = hook.posts();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(
        (&posts[0]["host"], &posts[0]["check"], &posts[0]["status"]),
        (&json!("r1"), &json!("disk"), &json!("up"))
    );
}

#[test]
fn a_failing_receiver_gets_3_tries_then_the_failure_is_logged_without_the_secret() {
    let hook = receiver(500);
    let serve = Serve::start("fail", &config(&[own("fail", "fall", "uudd")], &[]), &hook);
    wait("the failure line", || {
        serve.stderr().contains("notify failed")
    });
    settle();
    assert_eq!(hook.got().len(), 3);
    let err = serve.stderr();
    assert!(
        err.contains("notify failed: box/fall down: HTTP 500"),
        "{err}"
    );
    assert!(!err.contains(SECRET), "{err}");
}

#[test]
fn a_hung_host_does_not_delay_another_hosts_alert() {
    let hook = receiver(200);
    let hung = Fake::start(|_, _| Reply::Hang);
    let fast = Fake::start(host(&[("disk", "uudd")]));
    let started = Instant::now();
    let remotes = [
        ("hung", hung.url("/status"), 30),
        ("fast", fast.url("/status"), 5),
    ];
    let remotes: Vec<_> = remotes
        .iter()
        .map(|(n, u, t)| (*n, u.as_str(), *t))
        .collect();
    let _serve = Serve::start("hung", &config(&[], &remotes), &hook);
    wait("the fast alert", || !hook.got().is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(hook.posts()[0]["host"], "fast");
}

#[test]
fn serve_refuses_a_notify_file_others_can_read_a_missing_one_or_an_invalid_one() {
    let dir = scratch("refuse");
    std::fs::write(dir.join("monitors.json"), config(&[], &[])).unwrap();
    let notify = dir.join("notify.json");
    let run = || beacon(&dir).output().unwrap();
    let missing = run();
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("notify.json: "));
    let file = json!({ "url": "https://x/hooks/in/a", "secret": SECRET }).to_string();
    std::fs::write(&notify, &file).unwrap();
    std::fs::set_permissions(&notify, PermissionsExt::from_mode(0o644)).unwrap();
    let open = run();
    assert_eq!(open.status.code(), Some(2));
    let err = String::from_utf8_lossy(&open.stderr);
    assert!(
        err.contains("mode 644") && err.contains("chmod 600"),
        "{err}"
    );
    std::fs::set_permissions(&notify, PermissionsExt::from_mode(0o600)).unwrap();
    let slow = json!({ "url": "https://x/hooks/in/a", "secret": SECRET, "alive_every_s": 30 });
    std::fs::write(&notify, slow.to_string()).unwrap();
    let o = run();
    assert_eq!(o.status.code(), Some(2));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("alive_every_s 30: want 0 (off) or at least 60"),
        "{err}"
    );
    for bad in [
        format!("{{ \"secret\": \"{SECRET}\""),
        json!({ "url": 7, "secret": SECRET }).to_string(),
    ] {
        std::fs::write(&notify, bad).unwrap();
        let o = run();
        assert_eq!(o.status.code(), Some(2));
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains("want {\"url\""), "{err}");
        assert!(!err.contains(SECRET), "{err}");
    }
}
