use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const SPEEDUP: u64 = 60;
const WINDOW: Duration = Duration::from_secs(10);
const REAL_S: f64 = 600.0;
const DAY_SPEEDUP: u64 = 8640;
const CLIENTS: usize = 50;
const POLLS: usize = 60;
static ALONE: Mutex<()> = Mutex::new(());

struct Serve {
    child: Child,
    addr: String,
    config: String,
    origin: String,
    state: PathBuf,
}

impl Serve {
    fn start(
        name: &str,
        host_urls: &[String],
        checks: &[Value],
        speedup: u64,
        prefill: bool,
    ) -> Serve {
        let root = format!("{}/budget-{name}", env!("CARGO_TARGET_TMPDIR"));
        std::fs::remove_dir_all(&root).ok();
        let state = Path::new(&root).join("home/.local/state/beacon");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(format!("{root}/tmp")).unwrap();
        if prefill {
            std::fs::write(state.join("serve.log"), "old line\n".repeat(70_000)).unwrap();
            std::fs::write(state.join("stale.bin"), vec![0u8; 21 << 20]).unwrap();
        }
        let (origin, clone) = (format!("{root}/origin.git"), format!("{root}/clone"));
        git(&root, &["init", "-q", "--bare", &origin]);
        git(&root, &["clone", "-q", &origin, &clone]);
        let config = format!("{clone}/monitors.json");
        std::fs::write(&config, config_json(host_urls, checks)).unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-qm", "one"]);
        git(&clone, &["push", "-q", "origin", "HEAD"]);
        let log = std::fs::File::options()
            .append(true)
            .create(true)
            .open(state.join("serve.log"))
            .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_beacon"))
            .args(["serve", "--config", &config])
            .args(["--host", "box", "--listen", "127.0.0.1:0"])
            .env("BEACON_SPEEDUP", speedup.to_string())
            .env("HOME", format!("{root}/home"))
            .env("TMPDIR", format!("{root}/tmp"))
            .stdout(Stdio::piped())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let addr = line.split(' ').nth(3).unwrap().trim_end_matches(':').into();
        thread::spawn(move || std::io::copy(&mut stdout, &mut std::io::sink()));
        Serve {
            child,
            addr,
            config,
            origin,
            state,
        }
    }

    fn stop(mut self) -> Reading {
        let now = peek(self.child.id());
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        now
    }
}

#[derive(Debug)]
struct Reading {
    cpu_s: f64,
    peak_bytes: u64,
    wakeups: u64,
}

#[cfg(target_os = "linux")]
fn peek(pid: u32) -> Reading {
    let field = |text: &str, key: &str| -> u64 {
        let line = text.lines().find_map(|l| l.strip_prefix(key)).unwrap();
        line.trim().trim_end_matches("kB").trim().parse().unwrap()
    };
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let ticks: Vec<u64> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split(' ')
        .map(|f| f.parse().unwrap_or(0))
        .collect();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let switches = |t: std::fs::DirEntry| {
        let status = std::fs::read_to_string(t.path().join("status")).unwrap_or_default();
        field(&status, "voluntary_ctxt_switches:")
    };
    let tasks = std::fs::read_dir(format!("/proc/{pid}/task")).unwrap();
    Reading {
        cpu_s: f(ticks[11..15].iter().sum()) / 100.0,
        peak_bytes: field(&status, "VmHWM:") * 1024,
        wakeups: tasks.flatten().map(switches).sum(),
    }
}

#[cfg(target_os = "macos")]
fn peek(pid: u32) -> Reading {
    let u = beacon_sys::proc_usage(i32::try_from(pid).unwrap()).unwrap();
    Reading {
        cpu_s: f(u.cpu_ns) / 1e9,
        peak_bytes: u.peak_bytes,
        wakeups: u.wakeups,
    }
}

#[expect(
    clippy::as_conversions,
    reason = "test counts stay far below f64's 2^53 exact range"
)]
fn f(n: u64) -> f64 {
    n as f64
}

fn git(dir: &str, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C", dir])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn check(id: &str) -> Value {
    json!({ "id": id, "command": "true", "every_s": 60, "timeout_s": 5 })
}

fn config_json(host_urls: &[String], checks: &[Value]) -> String {
    let mut hosts = json!({ "box": { "url": "https://box/status", "checks": checks } });
    for (i, url) in host_urls.iter().enumerate() {
        hosts[format!("r{i}")] = json!({ "url": url, "checks": [check("c")] });
    }
    json!({ "version": 1, "monitors": [], "hosts": hosts }).to_string()
}

fn size(path: &Path) -> u64 {
    std::fs::read_dir(path).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| match e.metadata() {
                Ok(m) if m.is_dir() => size(&e.path()),
                Ok(m) => m.len(),
                Err(_) => 0,
            })
            .sum()
    })
}

fn fake_hosts() -> (Vec<String>, Arc<Mutex<Vec<usize>>>) {
    let polls = Arc::new(Mutex::new(Vec::new()));
    let urls = (0..5)
        .map(|_| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("https://{}/status", listener.local_addr().unwrap());
            let polls = polls.clone();
            thread::spawn(move || {
                for mut s in listener.incoming().flatten() {
                    let mut head = Vec::new();
                    let mut b = [0u8];
                    while !head.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap_or(0) == 1 {
                        head.push(b[0]);
                    }
                    let reply = b"HTTP/1.1 304 Not Modified\r\nETag: \"same\"\r\n\r\n";
                    s.write_all(reply).ok();
                    polls.lock().unwrap().push(head.len() + reply.len());
                }
            });
            url
        })
        .collect();
    (urls, polls)
}

fn etag(addr: &str, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    if let Err(e) = s.read_to_string(&mut raw) {
        panic!("{path}: no answer within 5 s: {e}");
    }
    raw.lines()
        .find_map(|l| l.strip_prefix("ETag: "))
        .unwrap()
        .into()
}

fn poll(conn: &mut BufReader<TcpStream>, path: &str, etag: &str) -> Option<(u16, usize)> {
    let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\nIf-None-Match: {etag}\r\n\r\n");
    conn.get_mut().write_all(req.as_bytes()).ok()?;
    let (mut head, mut line) = (String::new(), String::new());
    while line != "\r\n" {
        line.clear();
        if conn.read_line(&mut line).ok()? == 0 {
            return None;
        }
        head.push_str(&line);
    }
    Some((head.get(9..12)?.parse().ok()?, req.len() + head.len()))
}

fn idle() -> &'static (Reading, Vec<usize>, f64) {
    static IDLE: OnceLock<(Reading, Vec<usize>, f64)> = OnceLock::new();
    IDLE.get_or_init(|| {
        let (urls, polls) = fake_hosts();
        let serve = Serve::start("idle", &urls, &[check("a"), check("b")], SPEEDUP, false);
        let quiet = Serve::start("quiet", &[], &[], SPEEDUP, false);
        thread::sleep(Duration::from_secs(1));
        let pid = quiet.child.id();
        let before = peek(pid).wakeups;
        thread::sleep(WINDOW);
        let wakeups_a_minute = f(peek(pid).wakeups - before) * 60.0 / REAL_S;
        quiet.stop();
        let usage = serve.stop();
        eprintln!("idle over {REAL_S} real s: {usage:?}");
        let polls = polls.lock().unwrap().clone();
        (usage, polls, wakeups_a_minute)
    })
}

#[test]
fn serve_cpu_is_at_most_0_2_percent_of_a_core() {
    let pct = idle().0.cpu_s / REAL_S * 100.0;
    eprintln!("cpu {pct:.4}% of a core");
    assert!(pct <= 0.2, "cpu {pct:.4}% > 0.2%");
}

#[test]
fn serve_wakes_at_most_70_times_an_idle_minute() {
    let per_min = idle().2;
    eprintln!("{per_min:.1} wake-ups a minute");
    assert!(per_min <= 70.0, "{per_min:.1} wake-ups a minute > 70");
}

#[test]
fn serve_resident_peak_is_at_most_10_mb() {
    let rss = idle().0.peak_bytes;
    eprintln!("peak resident {rss} bytes");
    assert!(rss <= 10 << 20, "peak resident {rss} bytes > 10 MB");
}

#[test]
fn fake_hosts_see_polls_of_at_most_1_kb() {
    let polls = &idle().1;
    assert!(polls.iter().all(|&n| n <= 1024), "polls of {polls:?} bytes");
}

#[test]
fn a_flapping_day_keeps_logs_at_512_kb_state_at_20_mb_and_logs_each_outage_once() {
    let _alone = ALONE.lock().unwrap();
    let flap = json!({
        "id": "flap",
        "command": "[ $(( $(date +%s) % 2 )) = 0 ]",
        "every_s": 3600,
        "timeout_s": 5,
    });
    let serve = Serve::start("soak", &[], &[flap], DAY_SPEEDUP, true);
    let logs = |state: &Path| {
        ["serve.log", "serve.log.1"]
            .map(|f| std::fs::read_to_string(state.join(f)).unwrap_or_default())
            .concat()
    };
    let (off, started) = (format!("{}.off", serve.origin), Instant::now());
    let (mut outages, mut log_peak, mut state_peak) = (0, 0, 0);
    thread::sleep(Duration::from_millis(500));
    while started.elapsed() < WINDOW {
        let (from, to) = if Path::new(&serve.origin).exists() {
            outages += 1;
            (&serve.origin, &off)
        } else {
            (&off, &serve.origin)
        };
        std::fs::rename(from, to).unwrap();
        thread::sleep(Duration::from_millis(250));
        log_peak = log_peak.max(logs(&serve.state).len());
        state_peak = state_peak.max(size(&serve.state));
    }
    let state = serve.state.clone();
    serve.stop();
    let pull_errors = logs(&state).matches("git pull").count();
    eprintln!(
        "a simulated day: {outages} outages, {pull_errors} pull errors logged, logs peak {log_peak} bytes, state peak {state_peak} bytes"
    );
    assert!(log_peak <= 512 << 10, "logs {log_peak} bytes > 512 KB");
    assert!(state_peak <= 20 << 20, "state {state_peak} bytes > 20 MB");
    assert!(
        pull_errors <= outages,
        "{pull_errors} pull errors logged for {outages} outages"
    );
}

#[test]
fn status_and_metrics_p99_is_at_most_2_ms_for_50_clients_with_a_hung_host_and_a_slow_disk() {
    idle();
    let _alone = ALONE.lock().unwrap();
    let hung = TcpListener::bind("127.0.0.1:0").unwrap();
    let hung_addr = hung.local_addr().unwrap();
    thread::spawn(move || hung.incoming().flatten().collect::<Vec<_>>());
    let probe = json!({
        "id": "hung",
        "command": format!("curl -s -m 25 http://{hung_addr}/"),
        "every_s": 60,
        "timeout_s": 25,
    });
    let serve = Serve::start("loaded", &[], &[probe], 1, false);
    std::fs::remove_file(&serve.config).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&serve.config)
            .status()
            .unwrap()
            .success()
    );
    thread::sleep(Duration::from_millis(1500));
    let paths = ["/status", "/metrics", "/config"];
    let etags: Vec<_> = paths.iter().map(|p| etag(&serve.addr, p)).collect();
    let start = Arc::new(Barrier::new(CLIENTS));
    let clients: Vec<_> = (0..CLIENTS)
        .map(|_| {
            let (addr, etags, start) = (serve.addr.clone(), etags.clone(), start.clone());
            thread::spawn(move || {
                let conn = TcpStream::connect(addr).unwrap();
                conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut conn = BufReader::new(conn);
                poll(&mut conn, paths[0], &etags[0]);
                start.wait();
                let mut took = Vec::new();
                for i in 0..POLLS {
                    let (path, etag) = (paths[i % paths.len()], &etags[i % paths.len()]);
                    let t = Instant::now();
                    let reply = poll(&mut conn, path, etag);
                    took.push(t.elapsed());
                    let (code, bytes) = reply.unwrap_or((0, 0));
                    assert_eq!(code, 304, "{path}: unchanged poll");
                    assert!(bytes <= 1024, "{path}: {bytes} bytes both ways > 1 KB");
                }
                took
            })
        })
        .collect();
    let mut took: Vec<_> = clients
        .into_iter()
        .flat_map(|c| c.join().unwrap())
        .collect();
    let usage = serve.stop();
    took.sort();
    let p99 = took[took.len() * 99 / 100];
    eprintln!(
        "{} polls from {CLIENTS} clients: p99 {p99:?}, max {:?}, peak resident {} bytes",
        took.len(),
        took[took.len() - 1],
        usage.peak_bytes
    );
    assert!(p99 <= Duration::from_millis(2), "p99 {p99:?} > 2 ms");
    assert!(
        usage.peak_bytes <= 10 << 20,
        "peak resident {} bytes > 10 MB",
        usage.peak_bytes
    );
}
