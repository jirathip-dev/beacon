use crate::config::{HttpsUrl, NotifyPolicy};
use crate::net::agent;
use crate::out;
use crate::serve::{every, unix_now};
use crate::state::{self, Announced};
use crate::status::{Rendered, Status, rfc3339};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use ureq::{Agent, Body, http::Response};

pub const POLL_S: u64 = 60;
pub const CHECK_POLLS: u32 = 2;
pub const HOST_FAILS: u32 = 3;
pub const GROUP_S: u64 = 30;
const TRIES_S: [u64; 3] = [0, 5, 30];

/// Each watched host's name and whether it is down now.
pub type Down = Mutex<BTreeMap<String, bool>>;

/// The `--notify` file: the webhook URL, its HMAC secret and the `alive` interval.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    url: HttpsUrl,
    secret: String,
    alive_every_s: Option<u64>,
}

impl Notify {
    /// Reads a notify file that only its owner can read.
    pub fn load(path: &str) -> Result<Notify, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let mode = file.metadata().map_or(0, |m| m.permissions().mode()) & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!("{path}: mode {mode:o}; chmod 600 it"));
        }
        let notify: Notify = serde_json::from_reader(file).map_err(|e| {
            format!(
                "{path}: want {{\"url\": \"https://…\", \"secret\": \"…\"}} (line {} column {})",
                e.line(),
                e.column()
            )
        })?;
        if (1..60).contains(&notify.alive_every_s.unwrap_or(300)) {
            return Err(format!("{path}: alive_every_s: want 0 or at least 60"));
        }
        Ok(notify)
    }
}

/// `sha256=<hex HMAC-SHA256 of body under secret>`, the `X-Signature-256` value.
pub fn sign(secret: &[u8], body: &[u8]) -> String {
    let Ok(mut mac) = <Hmac<Sha256> as KeyInit>::new_from_slice(secret) else {
        return String::new();
    };
    mac.update(body);
    let mac = mac.finalize().into_bytes();
    let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256={hex}")
}

#[derive(Serialize, Clone, Debug)]
pub struct Event {
    kind: &'static str,
    host: String,
    check: Option<String>,
    status: Status,
    message: String,
    at: String,
}

impl Event {
    /// The `new job found` announcement of an unreviewed job (ADR D12 §6).
    pub fn job(host: &str, id: &str, status: Status, label: &str) -> Event {
        Event {
            kind: "job",
            host: host.into(),
            check: Some(id.into()),
            status,
            message: format!("new job found: {label}")
                .chars()
                .take(200)
                .collect(),
            at: rfc3339(unix_now()),
        }
    }
}

/// One host's last sent statuses and the polls that may change them.
#[derive(Default)]
pub struct Tracker {
    host: String,
    sent: BTreeMap<String, Status>,
    pending: BTreeMap<String, u32>,
    fails: u32,
    down: Option<bool>,
}

impl Tracker {
    pub fn new(host: &str) -> Tracker {
        let host = host.into();
        Tracker {
            host,
            ..Tracker::default()
        }
    }

    /// One poll of check `id`; the event once a new status holds for `CHECK_POLLS` polls.
    pub fn check(&mut self, id: &str, status: Status, message: &str) -> Option<Event> {
        let Some(sent) = self.sent.get_mut(id) else {
            self.sent.insert(id.into(), status);
            return None;
        };
        if *sent == status {
            self.pending.remove(id);
            return None;
        }
        let n = self.pending.entry(id.into()).or_default();
        *n += 1;
        if *n < CHECK_POLLS {
            return None;
        }
        *sent = status;
        self.pending.remove(id);
        Some(self.event(Some(id), status, message))
    }

    pub fn polled(&mut self, result: &Result<(), String>) -> Option<Event> {
        let Err(why) = result else {
            self.fails = 0;
            let was_down = self.down.replace(false) == Some(true);
            return was_down.then(|| self.event(None, Status::Up, "answering"));
        };
        self.fails += 1;
        if self.fails < HOST_FAILS || self.down == Some(true) {
            return None;
        }
        let was_up = self.down.replace(true) == Some(false);
        was_up.then(|| self.event(None, Status::Down, &format!("unreachable: {why}")))
    }

    fn event(&self, check: Option<&str>, status: Status, message: &str) -> Event {
        Event {
            kind: if check.is_some() { "check" } else { "host" },
            host: self.host.clone(),
            check: check.map(String::from),
            status,
            message: message.chars().take(200).collect(),
            at: rfc3339(unix_now()),
        }
    }
}

#[derive(Deserialize)]
struct Remote {
    checks: BTreeMap<String, Seen>,
}

#[derive(Deserialize)]
struct Seen {
    status: Status,
    message: String,
    name: Option<String>,
    group: Option<String>,
}

/// Polls `host` every `POLL_S` while `target` names its URL and timeout, sending each change on `tx`.
pub fn watch(
    host: &str,
    target: impl Fn() -> Option<(String, u8)>,
    tx: &Sender<Event>,
    down: &Down,
) {
    let (agent, mut tracker) = (agent(), Tracker::new(host));
    let (mut etag, mut last) = (String::new(), BTreeMap::<String, Seen>::new());
    let home = std::env::var_os("HOME");
    let mut announced = home.map(|h| Announced::load(&state::dir(Path::new(&h)), host));
    while let Some((url, timeout_s)) = target() {
        let got = agent
            .get(&url)
            .header("If-None-Match", &etag)
            .config()
            .timeout_global(Some(Duration::from_secs(timeout_s.into())))
            .build()
            .call();
        let polled = ok(got).and_then(|mut r| {
            if r.status() == 304 {
                return Ok(());
            }
            let tag = r.headers().get("etag").and_then(|v| v.to_str().ok());
            let tag = tag.unwrap_or_default().to_string();
            let body = r.body_mut().read_to_vec().map_err(|e| e.to_string())?;
            let body: Remote = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
            (etag, last) = (tag, body.checks);
            Ok(())
        });
        let mut events = vec![tracker.polled(&polled)];
        if polled.is_ok() {
            let unreviewed = || last.iter().filter(|(_, c)| c.group.is_none());
            let unreviewed = || unreviewed().filter_map(|(id, c)| Some((id, c, c.name.as_ref()?)));
            if let Some(a) = &mut announced {
                let fresh = a.admit(unreviewed().map(|(_, _, n)| n.clone()));
                let new = unreviewed().filter(|(_, _, n)| fresh.contains(n));
                events.extend(new.map(|(id, c, n)| Some(Event::job(host, id, c.status, n))));
            }
            events.extend(
                last.iter()
                    .map(|(id, c)| tracker.check(id, c.status, &c.message)),
            );
        }
        for e in events.into_iter().flatten() {
            tx.send(e).ok();
        }
        let is_down = tracker.down == Some(true);
        down.lock().map(|mut d| d.insert(host.into(), is_down)).ok();
        thread::sleep(every(POLL_S));
    }
    down.lock().map(|mut d| d.remove(host)).ok();
}

/// POSTs a signed `alive` for `host` every `alive_every_s`, once each; `hosts` gives (hosts, down).
pub fn alive(notify: &Notify, host: &str, hosts: impl Fn() -> (usize, usize)) {
    let every_s = notify.alive_every_s.unwrap_or(300);
    if every_s == 0 {
        return;
    }
    let (agent, mut failing) = (agent(), false);
    loop {
        thread::sleep(every(every_s));
        let (n, m) = hosts();
        let mut e = Tracker::new(host).event(None, Status::Up, &format!("{n} hosts, {m} down"));
        e.kind = "alive";
        let why = post(&agent, notify, &Rendered::json(&e).body).err();
        if let (Some(why), false) = (&why, failing) {
            out::err(&format!("notify alive failed: {why}"));
        }
        failing = why.is_some();
    }
}

fn post(agent: &Agent, notify: &Notify, body: &[u8]) -> Result<(), String> {
    let sent = agent
        .post(&notify.url.0)
        .header("Content-Type", "application/json")
        .header("X-Signature-256", &sign(notify.secret.as_bytes(), body))
        .config()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .send(body);
    ok(sent).map(drop)
}

type Key = (String, Option<String>);

/// The grouping window and the reminder policy, in the clock `Alerts` is given.
pub struct Timing {
    pub group: Duration,
    pub hour: Duration,
    pub policy: NotifyPolicy,
}

#[derive(Serialize)]
pub struct Batch {
    kind: &'static str,
    pub summary: String,
    pub changes: Vec<Event>,
    at: String,
}

/// Changes waiting for their window to close, and what is down with the reminders it got.
#[derive(Default)]
pub struct Alerts {
    opened: Option<Duration>,
    changes: Vec<Event>,
    down: BTreeMap<Key, (Duration, u32)>,
}

impl Alerts {
    pub fn change(&mut self, e: Event, now: Duration) {
        let key = (e.host.clone(), e.check.clone());
        if e.status == Status::Down {
            self.down.insert(key, (now, 0));
        } else {
            self.down.remove(&key);
        }
        self.opened.get_or_insert(now);
        self.changes.push(e);
    }

    /// The batch due at `now`: reminders that fell due join the window, sent once it has been open `group`.
    pub fn due(&mut self, now: Duration, t: &Timing) -> Option<Batch> {
        let hosts_down = self.hosts_down();
        for ((host, check), (since, sent)) in &mut self.down {
            let next = remind_at(*since, *sent, t);
            if next.is_none_or(|n| n > now) || check.as_ref().is_some() && hosts_down.contains(host)
            {
                continue;
            }
            *sent += 1;
            let name = check
                .as_ref()
                .map_or(host.clone(), |c| format!("{host}/{c}"));
            let hours = u32::from(t.policy.remind_every_h) * *sent;
            self.changes.push(Event {
                kind: "reminder",
                host: host.clone(),
                check: check.clone(),
                status: Status::Down,
                message: format!("still down: {name} for {hours}h"),
                at: rfc3339(unix_now()),
            });
            self.opened.get_or_insert(now);
        }
        if self.opened.is_none_or(|o| o + t.group > now) {
            return None;
        }
        self.opened = None;
        let mut changes = std::mem::take(&mut self.changes);
        changes.retain(|e| e.check.is_none() || !hosts_down.contains(&e.host));
        (!changes.is_empty()).then(|| Batch {
            kind: "batch",
            summary: summary(&changes),
            changes,
            at: rfc3339(unix_now()),
        })
    }

    /// How long until `due` may have something new; `None` when only a change can bring it.
    pub fn wait(&self, now: Duration, t: &Timing) -> Option<Duration> {
        let hosts_down = self.hosts_down();
        let reminders = self
            .down
            .iter()
            .filter_map(|((host, check), (since, sent))| {
                let swallowed = check.is_some() && hosts_down.contains(host);
                remind_at(*since, *sent, t).filter(|_| !swallowed)
            });
        let window = self.opened.map(|o| o + t.group);
        let next = reminders.chain(window).min()?;
        Some(next.saturating_sub(now))
    }

    fn hosts_down(&self) -> BTreeSet<String> {
        let hosts = self.down.keys().filter(|(_, check)| check.is_none());
        hosts.map(|(host, _)| host.clone()).collect()
    }
}

fn remind_at(since: Duration, sent: u32, t: &Timing) -> Option<Duration> {
    let every = t.hour.checked_mul(t.policy.remind_every_h.into())?;
    let n = (sent < t.policy.remind_max.into()).then_some(sent + 1)?;
    since.checked_add(every.checked_mul(n)?)
}

fn summary(changes: &[Event]) -> String {
    if let [e] = changes {
        return match (e.kind, &e.check) {
            ("reminder", _) => e.message.clone(),
            (_, Some(c)) => format!("{}/{c} {}", e.host, e.status),
            (_, None) => format!("{} {}", e.host, e.status),
        };
    }
    let mut parts: Vec<_> = changes
        .iter()
        .filter(|e| e.kind == "host")
        .map(|e| format!("{} {}", e.host, e.status))
        .collect();
    let count = |kind: &str, status: Status| {
        let of = |e: &&Event| e.kind == kind && e.status == status;
        changes.iter().filter(of).count()
    };
    for (n, what) in [
        (count("check", Status::Down), "down"),
        (count("check", Status::Up), "up"),
    ] {
        let s = if n == 1 { "" } else { "s" };
        if n > 0 {
            parts.push(format!("{n} check{s} {what}"));
        }
    }
    let reminded = count("reminder", Status::Down);
    if reminded > 0 {
        parts.push(format!("{reminded} still down"));
    }
    parts.join(", ")
}

/// Groups events into batches with `Alerts` and POSTs each, signed, with 3 tries; logs and drops it when all fail.
pub fn deliver(notify: &Notify, events: &Receiver<Event>, policy: impl Fn() -> NotifyPolicy) {
    let (agent, start, mut alerts) = (agent(), Instant::now(), Alerts::default());
    loop {
        let timing = Timing {
            group: every(GROUP_S),
            hour: every(3600),
            policy: policy(),
        };
        let got = match alerts.wait(start.elapsed(), &timing) {
            Some(wait) => events.recv_timeout(wait),
            None => events.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match got {
            Ok(e) => alerts.change(e, start.elapsed()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(batch) = alerts.due(start.elapsed(), &timing) else {
            continue;
        };
        let body = Rendered::json(&batch).body;
        let mut why = None;
        for wait in TRIES_S {
            thread::sleep(every(wait));
            why = post(&agent, notify, &body).err();
            if why.is_none() {
                break;
            }
        }
        if let Some(why) = why {
            out::err(&format!("notify failed: {}: {why}", batch.summary));
        }
    }
}

fn ok(sent: Result<Response<Body>, ureq::Error>) -> Result<Response<Body>, String> {
    let r = sent.map_err(|e| e.to_string())?;
    let code = r.status().as_u16();
    match code {
        200..300 | 304 => Ok(r),
        _ => Err(format!("HTTP {code}")),
    }
}
