use crate::budget::over_budget;
use crate::clock::{every, unix_now};
use crate::history::Times;
use crate::monitor::{self, Probed, Seen};
use crate::net;
use crate::status::Rendered;
use std::fs::{self, File};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::Arc;
use std::thread::{self, Scope};
use std::time::{Duration, Instant};

pub const EVERY_S: u64 = 60;
const PROBERS: usize = 4;

/// Probes `monitors` every 60 s on four threads, each writing its monitors' ring files under `dir`;
/// a thread ends when `record` returns false.
pub fn run<'s>(
    s: &'s Scope<'s, '_>,
    generation: u64,
    monitors: Vec<Arc<Probed>>,
    dir: Option<&'s Path>,
    record: impl Fn(u64, &Probed, &Seen, Rendered) -> bool + Send + Copy + 's,
) {
    let mut threads: Vec<Vec<_>> = (0..PROBERS).map(|_| Vec::new()).collect();
    for (i, m) in monitors.into_iter().enumerate() {
        let path = dir.map(|d| d.join(format!("{}.bin", m.id.0)));
        let ring = Times::load(path.as_ref().and_then(|p| fs::read(p).ok()).as_deref());
        let file = path.filter(|p| fs::write(p, ring.image()).is_ok());
        let file = file.and_then(|p| File::options().write(true).open(p).ok());
        if let Some(t) = threads.get_mut(i % PROBERS) {
            t.push((m, file, ring, None, None));
        }
    }
    for mut mine in threads.into_iter().filter(|t| !t.is_empty()) {
        s.spawn(move || {
            let (agent, started) = (net::agent(), Instant::now());
            for round in 1.. {
                for (m, file, ring, checked, cert) in &mut mine {
                    if checked.is_none_or(|t: Instant| t.elapsed() >= every(3600)) {
                        *checked = Some(Instant::now());
                        if let Some(c) = monitor::cert_expiry(&m.monitor) {
                            *cert = c;
                        }
                    }
                    let seen = monitor::probe(&agent, &m.monitor, *cert);
                    let changed = ring.put(unix_now() / 60, seen.ms);
                    if !record(generation, m, &seen, ring.render(&m.id)) {
                        return;
                    }
                    let Some(f) = file else { continue };
                    let (image, len) = (ring.image(), f.metadata().map_or(0, |m| m.len()));
                    for r in changed {
                        let at = u64::try_from(r.start).unwrap_or(0);
                        let bytes = image.get(r).unwrap_or_default();
                        let (bytes, at) = if over_budget("history") {
                            (image, len)
                        } else {
                            (bytes, at)
                        };
                        f.write_all_at(bytes, at).ok();
                    }
                }
                let next = every(EVERY_S).checked_mul(round).unwrap_or(Duration::MAX);
                thread::sleep(next.saturating_sub(started.elapsed()));
            }
        });
    }
}
