use crate::config::{self, Host, HostCheck, Id};
use crate::metrics::{EVERY_S, Ring};
use crate::probe;
use crate::sampler::Sampler;
use crate::status::{Rendered, StatusBody};
use std::path::Path;
use std::sync::Mutex;
use std::thread::{self, Scope};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Response, Server};

const PULL_EVERY: Duration = Duration::from_secs(300);

struct Cell(Mutex<Rendered>);

impl Cell {
    fn get(&self) -> Rendered {
        self.0.lock().unwrap().clone()
    }

    fn set(&self, r: Rendered) {
        *self.0.lock().unwrap() = r;
    }
}

struct Live {
    generation: u64,
    body: StatusBody,
}

/// Serves `/status`, `/metrics` and `/config` for `name` on `listen`; returns only when it cannot start.
pub fn serve(name: &str, path: &str, listen: &str) -> String {
    let (raw, host) = match std::fs::read(path).map_err(|e| e.to_string()) {
        Ok(raw) => match host_of(&raw, name) {
            Ok(host) => (raw, host),
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
    let disks: Vec<_> = host.disks.iter().map(|d| d.as_str()).collect();
    println!(
        "serve {name} on {addr}: {} checks ({}), metrics every {EVERY_S}s ({})",
        checks.len(),
        checks.join(", "),
        disks.join(", ")
    );
    let live = Mutex::new(Live {
        generation: 0,
        body: StatusBody::new(name),
    });
    let status = Cell(Mutex::new(live.lock().unwrap().body.render()));
    let metrics = Cell(Mutex::new(Ring::new().render(name, &host.disks)));
    let config = Cell(Mutex::new(Rendered::of(raw.clone())));
    thread::scope(|s| {
        run_checks(s, 0, host.checks.clone(), &live, &status);
        let (live, status, config) = (&live, &status, &config);
        s.spawn(move || {
            let dir = Path::new(path).parent().unwrap_or(Path::new(""));
            let git = dir.join(".git").exists();
            let (mut seen, mut pulled) = (Ok(raw), None::<Instant>);
            loop {
                if git && pulled.is_none_or(|p| p.elapsed() >= PULL_EVERY) {
                    pulled = Some(Instant::now());
                    let args = ["-C", &dir.to_string_lossy(), "pull", "--ff-only", "-q"];
                    if probe::output("git", &args, Duration::from_secs(60)).is_none() {
                        eprintln!("git pull --ff-only in {}: failed", dir.display());
                    }
                }
                let read = std::fs::read(path).map_err(|e| e.to_string());
                if read != seen {
                    seen = read.clone();
                    match read.and_then(|raw| host_of(&raw, name).map(|h| (raw, h))) {
                        Ok((raw, host)) => {
                            config.set(Rendered::of(raw));
                            let mut l = live.lock().unwrap();
                            l.generation += 1;
                            let ids: Vec<_> = host.checks.iter().map(|c| &c.id).collect();
                            l.body.keep(&ids);
                            status.set(l.body.render());
                            eprintln!("reload {path}: {} checks", ids.len());
                            run_checks(s, l.generation, host.checks.clone(), live, status);
                        }
                        Err(e) => eprintln!("{path}: {e}; serving the last valid config"),
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        });
        s.spawn(|| {
            let (mut sampler, mut ring, started) = (Sampler::new(), Ring::new(), Instant::now());
            loop {
                ring.push(sampler.sample(unix_now(), &host.disks));
                metrics.set(ring.render(name, &host.disks));
                let next =
                    Duration::from_secs((started.elapsed().as_secs() / EVERY_S + 1) * EVERY_S);
                thread::sleep(next.saturating_sub(started.elapsed()));
            }
        });
        for request in server.incoming_requests() {
            let cell = match request.url() {
                "/status" => status,
                "/metrics" => &metrics,
                "/config" => config,
                _ => {
                    let _ = request.respond(Response::empty(404));
                    continue;
                }
            };
            let r = cell.get();
            let header = |k: &str, v: &str| Header::from_bytes(k, v).unwrap();
            let fresh = !request
                .headers()
                .iter()
                .any(|h| h.field.equiv("If-None-Match") && h.value.as_str() == r.etag);
            let response = match fresh {
                true => Response::from_data(r.body.to_vec())
                    .with_header(header("Content-Type", "application/json")),
                false => Response::from_data(vec![]).with_status_code(304),
            };
            let _ = request.respond(
                response
                    .with_header(header("ETag", &r.etag))
                    .with_header(header("Cache-Control", "no-cache")),
            );
        }
        "listener closed".into()
    })
}

fn host_of(raw: &[u8], name: &str) -> Result<Host, String> {
    let text = std::str::from_utf8(raw).map_err(|e| e.to_string())?;
    let mut config = config::parse(text)?;
    Id::try_from(name.to_string())
        .ok()
        .and_then(|id| config.hosts.remove(&id))
        .ok_or(format!("no host {name:?} in hosts"))
}

fn run_checks<'s>(
    s: &'s Scope<'s, '_>,
    generation: u64,
    checks: Vec<HostCheck>,
    live: &'s Mutex<Live>,
    status: &'s Cell,
) {
    for c in checks {
        s.spawn(move || {
            loop {
                let (st, message) = probe::run(&c.command, Duration::from_secs(c.timeout_s.into()));
                let mut l = live.lock().unwrap();
                if l.generation != generation {
                    return;
                }
                if l.body.record(&c.id, st, message, unix_now()) {
                    status.set(l.body.render());
                }
                drop(l);
                thread::sleep(Duration::from_secs(c.every_s.into()));
            }
        });
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
