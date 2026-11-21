mod each;
mod http;
mod hub;
mod once;

use crate::config::MountPath;
use crate::metrics::EVERY_S;
use crate::notify::Notify;
use crate::out;
use hub::Hub;
use std::thread;
use tiny_http::Server;

const ANSWERERS: usize = 4;

/// Serves `/status`, `/metrics`, `/config` and `/history` for `name` on `listen`; returns only when it cannot start.
pub fn serve(name: &str, path: &str, listen: &str, notify: Option<Notify>) -> String {
    let hub = match Hub::open(name, path, notify) {
        Ok(hub) => hub,
        Err(e) => return e,
    };
    let server = match Server::http(listen) {
        Ok(s) => s,
        Err(e) => return format!("listen {listen}: {e}"),
    };
    let addr = server
        .server_addr()
        .to_ip()
        .map_or(listen.into(), |a| a.to_string());
    let config = hub.config();
    let checks: Vec<_> = config
        .host
        .checks
        .iter()
        .map(|c| format!("{} every {}s", c.id.as_str(), c.every_s))
        .collect();
    let disks: Vec<_> = config.host.disks.iter().map(MountPath::as_str).collect();
    out::out(&format!(
        "serve {name} on {addr}: {} checks ({}), metrics every {EVERY_S}s ({}), {} monitors probed",
        checks.len(),
        checks.join(", "),
        disks.join(", "),
        config.monitors.len()
    ));
    let (hub, server) = (&hub, &server);
    thread::scope(|s| {
        once::start(s, hub);
        each::start(s, hub, &config);
        for _ in 1..ANSWERERS {
            s.spawn(|| http::answer(server, hub));
        }
        http::answer(server, hub);
        "listener closed".into()
    })
}
