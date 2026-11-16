use crate::config::Host;
use crate::metrics::{EVERY_S, Ring};
use crate::probe;
use crate::sampler::Sampler;
use crate::status::{Rendered, StatusBody};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Response, Server};

struct Cell(Mutex<Rendered>);

impl Cell {
    fn get(&self) -> Rendered {
        self.0.lock().unwrap().clone()
    }

    fn set(&self, r: Rendered) {
        *self.0.lock().unwrap() = r;
    }
}

/// Serves `/status` and `/metrics` for `host` on `listen`; returns only when it cannot listen.
pub fn serve(name: &str, host: &Host, listen: &str) -> String {
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
    let body = Mutex::new(StatusBody::new(name));
    let status = Cell(Mutex::new(body.lock().unwrap().render()));
    let metrics = Cell(Mutex::new(Ring::new().render(name, &host.disks)));
    thread::scope(|s| {
        for c in &host.checks {
            let (body, status) = (&body, &status);
            s.spawn(move || {
                loop {
                    let (st, message) =
                        probe::run(&c.command, Duration::from_secs(c.timeout_s.into()));
                    let mut b = body.lock().unwrap();
                    if b.record(&c.id, st, message, unix_now()) {
                        status.set(b.render());
                    }
                    drop(b);
                    thread::sleep(Duration::from_secs(c.every_s.into()));
                }
            });
        }
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
                "/status" => &status,
                "/metrics" => &metrics,
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

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
