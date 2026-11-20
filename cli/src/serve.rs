use crate::config::MountPath;
use crate::config::{self, Host, HostCheck, Id};
use crate::jobs::{self, Manager};
use crate::metrics::{EVERY_S, Ring};
use crate::notify::{self, Down, Event, Notify, Tracker};
use crate::out;
use crate::probe;
use crate::sampler::Sampler;
use crate::state::{self, Announced};
use crate::status::{Rendered, StatusBody};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, Scope};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Response, Server};

const PULL_EVERY_S: u64 = 300;
const TIDY_EVERY_S: u64 = 60;
const ANSWERERS: usize = 4;

struct Cell(Mutex<Rendered>);

impl Cell {
    fn get(&self) -> Rendered {
        lock(&self.0).clone()
    }

    fn set(&self, r: Rendered) {
        *lock(&self.0) = r;
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Live {
    generation: u64,
    body: StatusBody,
    own: Tracker,
}

type Watched = Mutex<BTreeMap<Id, (u64, String, u8)>>;

/// Serves `/status`, `/metrics` and `/config` for `name` on `listen`; returns only when it cannot start.
pub fn serve(name: &str, path: &str, listen: &str, notify: Option<&Notify>) -> String {
    let (raw, (host, others)) = match std::fs::read(path).map_err(|e| e.to_string()) {
        Ok(raw) => match host_of(&raw, name) {
            Ok(hosts) => (raw, hosts),
            Err(e) => return format!("{path}: {e}"),
        },
        Err(e) => return format!("{path}: {e}"),
    };
    let server = match Server::http(listen) {
        Ok(s) => s,
        Err(e) => return format!("listen {listen}: {e}"),
    };
    let addr = server
        .server_addr()
        .to_ip()
        .map_or(listen.into(), |a| a.to_string());
    let checks: Vec<_> = host
        .checks
        .iter()
        .map(|c| format!("{} every {}s", c.id.as_str(), c.every_s))
        .collect();
    let disks: Vec<_> = host.disks.iter().map(MountPath::as_str).collect();
    out::out(&format!(
        "serve {name} on {addr}: {} checks ({}), metrics every {EVERY_S}s ({})",
        checks.len(),
        checks.join(", "),
        disks.join(", ")
    ));
    let live = Mutex::new(Live {
        generation: 0,
        body: StatusBody::new(name),
        own: Tracker::new(name),
    });
    let (tx, rx) = mpsc::channel();
    let tx = notify.is_some().then_some(tx);
    let (watched, down) = (Watched::default(), Down::default());
    let reviews = Mutex::new(host.jobs.clone());
    let status = Cell(Mutex::new(lock(&live).body.render()));
    let metrics = Cell(Mutex::new(Ring::new().render(name, &host.disks)));
    let config = Cell(Mutex::new(Rendered::of(raw.clone())));
    if over_budget("mem") {
        std::mem::forget(std::hint::black_box(vec![1u8; 20 << 20]));
    }
    thread::scope(|s| {
        if over_budget("cpu") {
            s.spawn(|| {
                loop {
                    std::hint::black_box(0);
                }
            });
        }
        if over_budget("wake") {
            s.spawn(|| {
                loop {
                    thread::sleep(Duration::from_millis(1));
                }
            });
        }
        let (tx, watched, down) = (tx.as_ref(), &watched, &down);
        if let Some(notify) = notify {
            s.spawn(move || notify::deliver(notify, rx));
            let hosts = || {
                (
                    lock(watched).len() + 1,
                    lock(down).values().filter(|d| **d).count(),
                )
            };
            s.spawn(move || notify::alive(notify, name, hosts));
        }
        run_checks(s, 0, host.checks.clone(), &live, &status, tx);
        watch(s, 0, others, watched, down, tx);
        let (live, status, config, reviews) = (&live, &status, &config, &reviews);
        s.spawn(move || {
            let state = std::env::var_os("HOME").map(|h| state::dir(Path::new(&h)));
            let mut announced = tx.and(state.as_deref()).map(|d| Announced::load(d, name));
            jobs::watch(
                &Manager::here(),
                || lock(reviews).clone(),
                |rows| {
                    if let (Some(a), Some(tx)) = (&mut announced, tx) {
                        let fresh = a.admit(
                            rows.iter()
                                .filter(|r| r.group.is_none())
                                .map(|r| r.label.clone()),
                        );
                        for r in rows.iter().filter(|r| fresh.contains(&r.label)) {
                            tx.send(Event::job(name, &r.id, r.status, &r.label)).ok();
                        }
                    }
                    let mut l = lock(live);
                    for r in &rows {
                        if let (Some(tx), Some(e)) = (tx, l.own.check(&r.id, r.status, &r.message))
                        {
                            tx.send(e).ok();
                        }
                    }
                    if l.body.set_jobs(&rows, unix_now()) {
                        status.set(l.body.render());
                    }
                },
            );
        });
        s.spawn(move || {
            let dir = Path::new(path).parent().unwrap_or(Path::new(""));
            let git = dir.join(".git").exists();
            let state = std::env::var_os("HOME").map(|h| state::dir(Path::new(&h)));
            let (mut seen, mut pulled, mut tidied) = (Ok(raw), None::<Instant>, None::<Instant>);
            let mut pull_down = false;
            loop {
                if let Some(state) = &state
                    && tidied.is_none_or(|t| t.elapsed() >= every(TIDY_EVERY_S))
                {
                    tidied = Some(Instant::now());
                    state::tidy(state);
                }
                if git && pulled.is_none_or(|p| p.elapsed() >= every(PULL_EVERY_S)) {
                    pulled = Some(Instant::now());
                    let args = ["-C", &dir.to_string_lossy(), "pull", "--ff-only", "-q"];
                    let failed = probe::output("git", &args, Duration::from_secs(60)).is_none();
                    if failed && (!pull_down || over_budget("repeat")) {
                        out::err(&format!("git pull --ff-only in {}: failed", dir.display()));
                    }
                    pull_down = failed;
                }
                let read = std::fs::read(path).map_err(|e| e.to_string());
                if read != seen {
                    seen.clone_from(&read);
                    match read.and_then(|raw| host_of(&raw, name).map(|h| (raw, h))) {
                        Ok((raw, (host, others))) => {
                            config.set(Rendered::of(raw));
                            *lock(reviews) = host.jobs.clone();
                            let mut l = lock(live);
                            l.generation += 1;
                            let ids: Vec<_> = host.checks.iter().map(|c| &c.id).collect();
                            l.body.keep(&ids);
                            status.set(l.body.render());
                            run_checks(s, l.generation, host.checks.clone(), live, status, tx);
                            watch(s, l.generation, others, watched, down, tx);
                            drop(l);
                            out::err(&format!("reload {path}: {} checks", ids.len()));
                        }
                        Err(e) => out::err(&format!("{path}: {e}; serving the last valid config")),
                    }
                }
                thread::sleep(every(1));
            }
        });
        s.spawn(|| {
            let (mut sampler, mut ring, started) = (Sampler::new(), Ring::new(), Instant::now());
            loop {
                ring.push(sampler.sample(unix_now(), &host.disks));
                metrics.set(ring.render(name, &host.disks));
                let period = every(EVERY_S);
                let n = started.elapsed().as_nanos() / period.as_nanos() + 1;
                let next = u32::try_from(n)
                    .ok()
                    .and_then(|n| period.checked_mul(n))
                    .unwrap_or(Duration::MAX);
                thread::sleep(next.saturating_sub(started.elapsed()));
            }
        });
        let (metrics, server) = (&metrics, &server);
        let answer = move || {
            for request in server.incoming_requests() {
                let cell = match request.url() {
                    "/status" => status,
                    "/metrics" => metrics,
                    "/config" => config,
                    _ => {
                        request.respond(Response::empty(404)).ok();
                        continue;
                    }
                };
                if over_budget("slow") {
                    thread::sleep(Duration::from_millis(3));
                }
                let r = cell.get();
                let fresh = over_budget("poll")
                    || !request
                        .headers()
                        .iter()
                        .any(|h| h.field.equiv("If-None-Match") && h.value.as_str() == r.etag);
                let (response, kind) = if fresh {
                    (Response::from_data(r.body.to_vec()), "application/json")
                } else {
                    (Response::from_data(vec![]).with_status_code(304), "")
                };
                let headers = [
                    ("Content-Type", kind),
                    ("ETag", &r.etag),
                    ("Cache-Control", "no-cache"),
                ];
                let response = headers
                    .into_iter()
                    .filter(|(_, v)| !v.is_empty())
                    .filter_map(|(k, v)| Header::from_bytes(k, v).ok())
                    .fold(response, Response::with_header);
                request.respond(response).ok();
            }
        };
        for _ in 1..ANSWERERS {
            s.spawn(answer);
        }
        answer();
        "listener closed".into()
    })
}

fn host_of(raw: &[u8], name: &str) -> Result<(Host, BTreeMap<Id, Host>), String> {
    let text = std::str::from_utf8(raw).map_err(|e| e.to_string())?;
    let mut config = config::parse(text)?;
    Id::try_from(name.to_string())
        .ok()
        .and_then(|id| config.hosts.remove(&id))
        .map(|host| (host, config.hosts))
        .ok_or(format!("no host {name:?} in hosts"))
}

fn watch<'s>(
    s: &'s Scope<'s, '_>,
    generation: u64,
    others: BTreeMap<Id, Host>,
    watched: &'s Watched,
    down: &'s Down,
    tx: Option<&'s Sender<Event>>,
) {
    let Some(tx) = tx else { return };
    let mut w = lock(watched);
    let old = std::mem::take(&mut *w);
    for (id, h) in others {
        let born = old.get(&id).map_or(generation, |o| o.0);
        w.insert(id.clone(), (born, h.url.0, h.timeout_s));
        if born == generation {
            s.spawn(move || {
                let target = || {
                    let w = lock(watched);
                    let entry = w.get(&id).filter(|e| e.0 == born);
                    entry.map(|e| (e.1.clone(), e.2))
                };
                notify::watch(id.as_str(), target, tx, down);
            });
        }
    }
}

fn run_checks<'s>(
    s: &'s Scope<'s, '_>,
    generation: u64,
    checks: Vec<HostCheck>,
    live: &'s Mutex<Live>,
    status: &'s Cell,
    tx: Option<&'s Sender<Event>>,
) {
    for c in checks {
        s.spawn(move || {
            loop {
                let held = over_budget("lock").then(|| lock(&status.0));
                let (st, message) = probe::run(&c.command, Duration::from_secs(c.timeout_s.into()));
                drop(held);
                let mut l = lock(live);
                if l.generation != generation {
                    return;
                }
                if let Some(tx) = tx
                    && let Some(e) = l.own.check(c.id.as_str(), st, &message)
                {
                    tx.send(e).ok();
                }
                if l.body.record(&c.id, st, message, unix_now()) {
                    status.set(l.body.render());
                }
                drop(l);
                thread::sleep(every(c.every_s.into()));
            }
        });
    }
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn every(secs: u64) -> Duration {
    static SPEEDUP: OnceLock<u32> = OnceLock::new();
    let k = SPEEDUP.get_or_init(|| {
        let k = std::env::var("BEACON_SPEEDUP")
            .ok()
            .and_then(|k| k.parse().ok());
        k.filter(|&k| k > 0).unwrap_or(1)
    });
    Duration::from_secs(secs) / *k
}

pub(crate) fn over_budget(line: &str) -> bool {
    cfg!(feature = "over-budget") && std::env::var("BEACON_OVER_BUDGET").is_ok_and(|v| v == line)
}
