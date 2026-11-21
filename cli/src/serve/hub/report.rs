use super::{Hub, Live, Of, lock};
use crate::clock::unix_now;
use crate::config::Id;
use crate::history::Key;
use crate::jobs::Row;
use crate::notify::Event;
use crate::status::Status;

impl Hub {
    /// One result of a check, rule or monitor: to the notifier, the history and `/status`.
    /// False when `generation` is over: the caller's thread ends.
    pub fn report(&self, generation: u64, of: Of, status: Status, message: String) -> bool {
        let mut l = lock(&self.live);
        if l.config.generation != generation {
            return false;
        }
        let (tracked, key) = match &of {
            Of::Check(id) => (id.as_str().to_string(), Key::Check(id)),
            Of::Monitor(m, _) => {
                let (group, name) = m.monitor.names();
                (format!("{group}/{name}"), Key::Monitor(&m.id))
            }
        };
        self.track(&mut l, &tracked, status, &message);
        let now = unix_now();
        if l.history.record(key, status, &message, now) {
            self.publish("/history", l.history.render());
        }
        match of {
            Of::Check(id) if l.body.record(id, status, message, now) => {
                self.publish("/status", l.body.render());
            }
            Of::Check(_) => {}
            Of::Monitor(m, times) => {
                drop(l);
                self.publish(&format!("/history/{}", m.id.0), times);
            }
        }
        true
    }

    fn track(&self, l: &mut Live, id: &str, status: Status, message: &str) {
        if self.notifier.is_some()
            && let Some(e) = l.own.check(id, status, message)
        {
            self.send(e);
        }
    }

    /// This minute's job rows; they replace the last ones.
    pub fn report_jobs(&self, rows: &[Row]) {
        let mut l = lock(&self.live);
        for r in rows {
            self.track(&mut l, &r.id, r.status, &r.message);
            if let Ok(id) = Id::try_from(r.id.clone())
                && l.history
                    .record(Key::Check(&id), r.status, &r.message, unix_now())
            {
                self.publish("/history", l.history.render());
            }
        }
        if l.body.set_jobs(rows, unix_now()) {
            self.publish("/status", l.body.render());
        }
    }

    /// Queues one event for delivery; nothing without `--notify`.
    pub fn send(&self, event: Event) {
        if let Some(n) = &self.notifier {
            n.events.send(event).ok();
        }
    }
}
