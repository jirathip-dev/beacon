use super::{Hub, Loaded, lock};
use crate::config::Id;
use crate::notify;

impl Hub {
    /// Delivers the queued events until serve exits; nothing without `--notify`.
    pub fn deliver(&self) {
        if let Some(n) = &self.notifier {
            notify::deliver(&n.file, &lock(&n.inbox), || self.config().policy);
        }
    }

    /// The hosts known with `--notify`, this one included, and how many of them are down.
    pub fn hosts(&self) -> (usize, usize) {
        self.notifier.as_ref().map_or((1, 0), |n| {
            let down = lock(&n.down).values().filter(|d| **d).count();
            (lock(&n.watched).len() + 1, down)
        })
    }

    /// Takes the other hosts of `config` as the watched ones; returns those that need a new watcher.
    pub fn watched(&self, config: &Loaded) -> Vec<Id> {
        let Some(n) = &self.notifier else {
            return Vec::new();
        };
        let mut w = lock(&n.watched);
        let old = std::mem::take(&mut *w);
        let mut fresh = Vec::new();
        for (id, h) in &config.others {
            let born = old.get(id).map_or(config.generation, |o| o.0);
            w.insert(id.clone(), (born, h.url.0.clone(), h.timeout_s));
            if born == config.generation {
                fresh.push(id.clone());
            }
        }
        fresh
    }

    /// Watches host `id` while the watched hosts name it as taken at generation `born`.
    pub fn watch(&self, id: &Id, born: u64) {
        let Some(n) = &self.notifier else { return };
        let target = || {
            let w = lock(&n.watched);
            let entry = w.get(id).filter(|e| e.0 == born);
            entry.map(|e| (e.1.clone(), e.2))
        };
        notify::watch(id.as_str(), target, &n.events, &n.down);
    }
}
