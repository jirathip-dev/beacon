use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SPEEDUP: u64 = 60;
const WINDOW: Duration = Duration::from_secs(10);
const REAL_S: f64 = (WINDOW.as_secs() * SPEEDUP) as f64;
const DAY_S: f64 = 86400.0;

#[repr(C)]
#[derive(Default)]
struct Timeval {
    sec: i64,
    usec: Usec,
}

#[cfg(target_os = "macos")]
type Usec = i32;
#[cfg(not(target_os = "macos"))]
type Usec = i64;

#[repr(C)]
#[derive(Default)]
struct Rusage {
    utime: Timeval,
    stime: Timeval,
    _unused: [i64; 14],
}

unsafe extern "C" {
    fn wait4(pid: i32, status: *mut i32, options: i32, rusage: *mut Rusage) -> i32;
}

#[derive(Debug)]
struct Usage {
    cpu_s: f64,
    rss_bytes: u64,
    wakeups: u64,
    log_bytes: usize,
    state_bytes: u64,
}

struct Serve {
    child: Child,
    addr: String,
    root: String,
    state_before: u64,
    logs: Vec<JoinHandle<usize>>,
}

impl Serve {
    fn start(name: &str, host_urls: &[String], speedup: u64) -> Serve {
        let root = format!("{}/budget-{name}", env!("CARGO_TARGET_TMPDIR"));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["home", "tmp"] {
            std::fs::create_dir_all(format!("{root}/{d}")).unwrap();
        }
        let (origin, clone) = (format!("{root}/origin.git"), format!("{root}/clone"));
        git(&root, &["init", "-q", "--bare", &origin]);
        git(&root, &["clone", "-q", &origin, &clone]);
        std::fs::write(format!("{clone}/monitors.json"), config(host_urls)).unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-qm", "one"]);
        git(&clone, &["push", "-q", "origin", "HEAD"]);
        let state_before = state(&root);
        let mut child = Command::new(env!("CARGO_BIN_EXE_beacon"))
            .args(["serve", "--config", &format!("{clone}/monitors.json")])
            .args(["--host", "box", "--listen", "127.0.0.1:0"])
            .env("BEACON_SPEEDUP", speedup.to_string())
            .env("HOME", format!("{root}/home"))
            .env("TMPDIR", format!("{root}/tmp"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let addr = line.split(' ').nth(3).unwrap().trim_end_matches(':').into();
        let count = |mut r: Box<dyn Read + Send>, first: usize| {
            thread::spawn(move || {
                first + std::io::copy(&mut r, &mut std::io::sink()).unwrap() as usize
            })
        };
        let logs = vec![
            count(Box::new(stdout), line.len()),
            count(Box::new(child.stderr.take().unwrap()), 0),
        ];
        Serve {
            child,
            addr,
            root,
            state_before,
            logs,
        }
    }

    fn stop(mut self) -> Usage {
        let pid = self.child.id() as i32;
        let (rss_bytes, wakeups) = peek(pid);
        self.child.kill().unwrap();
        let (mut status, mut ru) = (0, Rusage::default());
        // SAFETY: `pid` is this process's unwaited child; `status` and `ru` are owned, writable and C-shaped.
        assert_eq!(unsafe { wait4(pid, &mut status, 0, &mut ru) }, pid);
        let secs = |t: &Timeval| t.sec as f64 + t.usec as f64 / 1e6;
        Usage {
            cpu_s: secs(&ru.utime) + secs(&ru.stime),
            rss_bytes,
            wakeups,
            log_bytes: self.logs.into_iter().map(|h| h.join().unwrap()).sum(),
            state_bytes: state(&self.root).saturating_sub(self.state_before),
        }
    }
}

#[cfg(target_os = "linux")]
fn peek(pid: i32) -> (u64, u64) {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let hwm = status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .unwrap();
    let kb: u64 = hwm.trim().trim_end_matches("kB").trim().parse().unwrap();
    let switches = |t: std::fs::DirEntry| {
        let status = std::fs::read_to_string(t.path().join("status")).unwrap_or_default();
        let n = status
            .lines()
            .find_map(|l| l.strip_prefix("voluntary_ctxt_switches:"));
        n.map_or(0, |n| n.trim().parse::<u64>().unwrap())
    };
    let tasks = std::fs::read_dir(format!("/proc/{pid}/task")).unwrap();
    (kb * 1024, tasks.flatten().map(switches).sum())
}

#[cfg(target_os = "macos")]
fn peek(pid: i32) -> (u64, u64) {
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    }
    let mut info = [0u64; 64];
    // SAFETY: flavor 4 writes one rusage_info_v4, smaller than the 512 bytes of `info`.
    assert_eq!(unsafe { proc_pid_rusage(pid, 4, info.as_mut_ptr()) }, 0);
    (info[30], info[4] + info[5])
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

fn config(host_urls: &[String]) -> String {
    let check = |id: &str| json!({ "id": id, "command": "true", "every_s": 60, "timeout_s": 5 });
    let mut hosts =
        json!({ "box": { "url": "https://box/status", "checks": [check("a"), check("b")] } });
    for (i, url) in host_urls.iter().enumerate() {
        hosts[format!("r{i}")] = json!({ "url": url, "checks": [check("c")] });
    }
    json!({ "version": 1, "monitors": [], "hosts": hosts }).to_string()
}

fn state(dir: &str) -> u64 {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| match e.file_type().unwrap().is_dir() {
                true => state(&e.path().to_string_lossy()),
                false => e.metadata().map_or(0, |m| m.len()),
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
                    let _ = s.write_all(reply);
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
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    raw.lines()
        .find_map(|l| l.strip_prefix("ETag: "))
        .unwrap()
        .into()
}

fn poll(conn: &mut BufReader<TcpStream>, path: &str, etag: &str) -> (u16, usize) {
    let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\nIf-None-Match: {etag}\r\n\r\n");
    conn.get_mut().write_all(req.as_bytes()).unwrap();
    let (mut head, mut line) = (String::new(), String::new());
    while line != "\r\n" {
        line.clear();
        conn.read_line(&mut line).unwrap();
        head.push_str(&line);
    }
    (head[9..12].parse().unwrap(), req.len() + head.len())
}

fn idle() -> &'static (Usage, Vec<usize>) {
    static IDLE: OnceLock<(Usage, Vec<usize>)> = OnceLock::new();
    IDLE.get_or_init(|| {
        let (urls, polls) = fake_hosts();
        let serve = Serve::start("idle", &urls, SPEEDUP);
        thread::sleep(WINDOW);
        let usage = serve.stop();
        eprintln!("idle over {REAL_S} real s: {usage:?}");
        let polls = polls.lock().unwrap().clone();
        (usage, polls)
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
    let per_min = idle().0.wakeups as f64 * 60.0 / REAL_S;
    eprintln!("{per_min:.1} wake-ups a minute");
    assert!(per_min <= 70.0, "{per_min:.1} wake-ups a minute > 70");
}

#[test]
fn serve_logs_at_most_512_kb_a_day() {
    let per_day = idle().0.log_bytes as f64 * DAY_S / REAL_S;
    eprintln!("logs {per_day:.0} bytes a day");
    assert!(
        per_day <= 512.0 * 1024.0,
        "logs {per_day:.0} bytes a day > 512 KB"
    );
}

#[test]
fn serve_state_grows_at_most_20_mb_a_day() {
    let per_day = idle().0.state_bytes as f64 * DAY_S / REAL_S;
    eprintln!("state {per_day:.0} bytes a day");
    assert!(
        per_day <= 20.0 * 1024.0 * 1024.0,
        "state {per_day:.0} bytes a day > 20 MB"
    );
}

#[test]
fn serve_resident_peak_is_at_most_10_mb() {
    let rss = idle().0.rss_bytes;
    eprintln!("peak resident {rss} bytes");
    assert!(rss <= 10 << 20, "peak resident {rss} bytes > 10 MB");
}

#[test]
fn fake_hosts_see_polls_of_at_most_1_kb() {
    let polls = &idle().1;
    assert!(polls.iter().all(|&n| n <= 1024), "polls of {polls:?} bytes");
}

#[test]
fn unchanged_polls_are_304s_under_1_kb_with_p99_at_most_2_ms() {
    idle();
    let serve = Serve::start("polled", &[], 1);
    thread::sleep(Duration::from_secs(1));
    let paths = ["/status", "/metrics", "/config"];
    let etags: Vec<_> = paths.iter().map(|p| etag(&serve.addr, p)).collect();
    let mut conn = BufReader::new(TcpStream::connect(&serve.addr).unwrap());
    let mut round = || {
        let mut took = Vec::new();
        for _ in 0..100 {
            for (path, etag) in paths.iter().zip(&etags) {
                let t = Instant::now();
                let (code, bytes) = poll(&mut conn, path, etag);
                took.push(t.elapsed());
                assert_eq!(code, 304, "{path}: unchanged poll");
                assert!(bytes <= 1024, "{path}: {bytes} bytes both ways > 1 KB");
            }
        }
        took.sort();
        took[took.len() * 99 / 100]
    };
    let p99 = (0..5).map(|_| round()).min().unwrap();
    let usage = serve.stop();
    eprintln!("p99 {p99:?}, peak resident {} bytes", usage.rss_bytes);
    assert!(p99 <= Duration::from_millis(2), "p99 {p99:?} > 2 ms");
    assert!(
        usage.rss_bytes <= 10 << 20,
        "peak resident {} bytes > 10 MB",
        usage.rss_bytes
    );
}
