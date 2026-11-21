mod notifier;
mod open;
mod report;

use crate::budget::over_budget;
use crate::clock::unix_now;
use crate::config::{Host, Id, NotifyPolicy, RuleCheck};
use crate::history::History;
use crate::monitor::Probed;
use crate::notify::{Down, Event, Notify, Tracker};
use crate::status::{Rendered, StatusBody};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// One valid config as serve uses it; `generation` counts the valid configs since start, from 1.
pub struct Loaded {
    pub generation: u64,
    pub raw: Vec<u8>,
    pub host: Host,
    pub others: BTreeMap<Id, Host>,
    pub monitors: Vec<Arc<Probed>>,
    pub rules: Vec<RuleCheck>,
    pub policy: NotifyPolicy,
}

/// What a result is of: a check or a rule, or a monitor with its `/history/<id>` body.
pub enum Of<'a> {
    Check(&'a Id),
    Monitor(&'a Probed, Rendered),
}

/// What `--notify` adds: where events go and what the host watchers share.
pub struct Notifier {
    pub file: Notify,
    down: Down,
    watched: Mutex<BTreeMap<Id, (u64, String, u8)>>,
    events: Sender<Event>,
    inbox: Mutex<Receiver<Event>>,
}

/// All state that two of serve's threads share; `saved` is the `ETag` of the history file at start.
pub struct Hub {
    pub name: String,
    pub path: String,
    pub state: Option<PathBuf>,
    pub times: Option<PathBuf>,
    pub saved: String,
    pub notifier: Option<Notifier>,
    live: Mutex<Live>,
    board: Mutex<BTreeMap<String, Rendered>>,
}

struct Live {
    config: Arc<Loaded>,
    body: StatusBody,
    own: Tracker,
    history: History,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Hub {
    /// The config that is live now.
    pub fn config(&self) -> Arc<Loaded> {
        Arc::clone(&lock(&self.live).config)
    }

    /// The history's minute tick; returns the `/history` body to save.
    pub fn tick(&self) -> Rendered {
        let mut l = lock(&self.live);
        if l.history.tick(unix_now()) {
            self.publish("/history", l.history.render());
        }
        self.body("/history").unwrap_or_else(|| l.history.render())
    }

    /// Runs `f`, holding the board while the budget tests' `lock` switch is on.
    pub fn hold<T>(&self, f: impl FnOnce() -> T) -> T {
        let held = over_budget("lock").then(|| lock(&self.board));
        let r = f();
        drop(held);
        r
    }

    /// Makes `body` the answer to `GET <path>`.
    pub fn publish(&self, path: &str, body: Rendered) {
        lock(&self.board).insert(path.into(), body);
    }

    /// The answer to `GET <path>`; `None` is a 404.
    pub fn body(&self, path: &str) -> Option<Rendered> {
        lock(&self.board).get(path).cloned()
    }
}
