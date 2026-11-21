use super::super::each;
use super::{Hub, Live, Loaded, Notifier, lock};
use crate::clock::unix_now;
use crate::config::{self, Id};
use crate::history::History;
use crate::monitor;
use crate::notify::{Down, Notify, Tracker};
use crate::out;
use crate::state;
use crate::status::{Rendered, StatusBody};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::Scope;

impl Hub {
    /// Reads and validates the config: generation 1. The error is serve's exit message.
    pub fn open(name: &str, path: &str, notify: Option<Notify>) -> Result<Hub, String> {
        let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let config = Arc::new(loaded(1, raw, name).map_err(|e| format!("{path}: {e}"))?);
        let state = std::env::var_os("HOME").map(|h| state::dir(Path::new(&h)));
        let times = state.as_deref().map(state::times);
        times.as_ref().map(std::fs::create_dir_all);
        let events = state.as_deref().and_then(state::events);
        let history = match &events {
            Some((b, at)) => History::load(name, b, *at).unwrap_or_else(|| {
                out::err("history/events.json: unreadable; starting an empty history");
                History::new(name, 0)
            }),
            None => History::new(name, 0),
        };
        let notifier = notify.map(|file| {
            let (events, inbox) = mpsc::channel();
            let (down, watched) = (Down::default(), Mutex::default());
            let inbox = Mutex::new(inbox);
            Notifier {
                file,
                down,
                watched,
                events,
                inbox,
            }
        });
        let live = Live {
            config: Arc::clone(&config),
            body: StatusBody::new(name),
            own: Tracker::new(name),
            history,
        };
        let (name, path) = (name.into(), path.into());
        let saved = events.map_or(String::new(), |(b, _)| Rendered::of(b).etag);
        let (live, board) = (Mutex::new(live), Mutex::default());
        let hub = Hub {
            name,
            path,
            state,
            times,
            saved,
            notifier,
            live,
            board,
        };
        hub.apply(&mut lock(&hub.live), config);
        Ok(hub)
    }

    /// A new valid config: the next generation, its parts started; returns the number of checks.
    pub fn load<'s>(&'s self, s: &'s Scope<'s, '_>, raw: Vec<u8>) -> Result<usize, String> {
        let config = Arc::new(loaded(self.config().generation + 1, raw, &self.name)?);
        let n = self.apply(&mut lock(&self.live), Arc::clone(&config));
        each::start(s, self, &config);
        Ok(n)
    }

    fn apply(&self, l: &mut Live, config: Arc<Loaded>) -> usize {
        let rules = config.rules.iter().map(|r| &r.id);
        let ids: Vec<_> = config
            .host
            .checks
            .iter()
            .map(|c| &c.id)
            .chain(rules)
            .collect();
        let monitors = config.monitors.iter();
        let tracks: Vec<_> = monitors
            .map(|m| (&m.id, m.monitor.names().0, m.monitor.names().1))
            .collect();
        l.body.keep(&ids);
        l.history.keep(&ids, &tracks, unix_now());
        self.publish("/config", Rendered::of(config.raw.clone()));
        self.publish("/status", l.body.render());
        self.publish("/history", l.history.render());
        let ours = |id: &str| config.monitors.iter().any(|m| m.id.0 == id);
        let gone = |path: &String| path.strip_prefix("/history/").is_some_and(|id| !ours(id));
        lock(&self.board).retain(|path, _| !gone(path));
        let n = ids.len();
        l.config = config;
        n
    }
}

fn loaded(generation: u64, raw: Vec<u8>, name: &str) -> Result<Loaded, String> {
    let text = std::str::from_utf8(&raw).map_err(|e| e.to_string())?;
    let mut config = config::parse(text)?;
    let host = Id::try_from(name.to_string())
        .ok()
        .and_then(|id| config.hosts.remove(&id))
        .ok_or(format!("no host {name:?} in hosts"))?;
    let others = std::mem::take(&mut config.hosts);
    let (policy, rules) = (config.notify, host.rule_checks().unwrap_or_default());
    let monitors = monitor::probed(config, name)
        .into_iter()
        .map(Arc::new)
        .collect();
    Ok(Loaded {
        generation,
        raw,
        host,
        others,
        monitors,
        rules,
        policy,
    })
}
