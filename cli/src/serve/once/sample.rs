use super::super::hub::{Hub, Of};
use crate::clock::{every, unix_now};
use crate::config::Id;
use crate::metrics::{EVERY_S, Ring};
use crate::rules::{self, Streak};
use crate::sampler::Sampler;
use std::collections::BTreeMap;
use std::thread::{self, Scope};
use std::time::{Duration, Instant};

/// The sampler every `EVERY_S`: the metric rules judged and reported, `/metrics` published.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub) {
    let first = hub.config();
    hub.publish("/metrics", Ring::new().render(&hub.name, &first.host.disks));
    s.spawn(move || {
        let (mut sampler, mut ring, started) = (Sampler::new(), Ring::new(), Instant::now());
        let cores = thread::available_parallelism().map_or(1, usize::from);
        let cores = u32::try_from(cores).map_or(1.0, f64::from);
        let mut streaks = BTreeMap::<Id, Streak>::new();
        let disks = &first.host.disks;
        loop {
            let sample = sampler.sample(unix_now(), disks);
            let config = hub.config();
            streaks.retain(|id, _| config.rules.iter().any(|r| r.id == *id));
            for r in &config.rules {
                let streak = streaks.entry(r.id.clone()).or_default();
                let (st, message) = rules::judge(r, &sample, cores, streak);
                hub.report(config.generation, Of::Check(&r.id), st, message);
            }
            ring.push(sample);
            hub.publish("/metrics", ring.render(&hub.name, disks));
            let period = every(EVERY_S);
            let n = started.elapsed().as_nanos() / period.as_nanos() + 1;
            let next = u32::try_from(n)
                .ok()
                .and_then(|n| period.checked_mul(n))
                .unwrap_or(Duration::MAX);
            thread::sleep(next.saturating_sub(started.elapsed()));
        }
    });
}
