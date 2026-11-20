use crate::config::HttpsUrl;
use crate::out;
use crate::serve::{every, unix_now};
use crate::state::{self, Announced};
use crate::status::{Rendered, Status, rfc3339};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use ureq::tls::{PemItem, RootCerts, TlsConfig, parse_pem};
use ureq::{Agent, Body, http::Response};

pub const POLL_S: u64 = 60;
pub const CHECK_POLLS: u32 = 2;
pub const HOST_FAILS: u32 = 3;
const TRIES_S: [u64; 3] = [0, 5, 30];

/// The `--notify` file: the webhook URL and its HMAC secret.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    url: HttpsUrl,
    secret: String,
}

impl Notify {
    /// Reads a notify file that only its owner can read.
    pub fn load(path: &str) -> Result<Notify, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let mode = file.metadata().map_or(0, |m| m.permissions().mode()) & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!("{path}: mode {mode:o}; chmod 600 it"));
        }
        serde_json::from_reader(file).map_err(|e| {
            format!(
                "{path}: want {{\"url\": \"https://…\", \"secret\": \"…\"}} (line {} column {})",
                e.line(),
                e.column()
            )
        })
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

#[derive(Serialize)]
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

    fn polled(&mut self, result: &Result<(), String>) -> Option<Event> {
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
pub fn watch(host: &str, target: impl Fn() -> Option<(String, u8)>, tx: &Sender<Event>) {
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
        thread::sleep(every(POLL_S));
    }
}

/// POSTs each event, signed, with 3 tries; logs and drops it when all fail.
pub fn deliver(notify: &Notify, events: Receiver<Event>) {
    let agent = agent();
    for e in events {
        let body = Rendered::json(&e).body;
        let signature = sign(notify.secret.as_bytes(), &body);
        let mut why = None;
        for wait in TRIES_S {
            thread::sleep(every(wait));
            let sent = agent
                .post(&notify.url.0)
                .header("Content-Type", "application/json")
                .header("X-Signature-256", &signature)
                .config()
                .timeout_global(Some(Duration::from_secs(10)))
                .build()
                .send(&body[..]);
            why = ok(sent).err();
            if why.is_none() {
                break;
            }
        }
        if let Some(why) = why {
            let (host, check) = (&e.host, e.check.as_deref().unwrap_or("-"));
            out::err(&format!(
                "notify failed: {host}/{check} {}: {why}",
                e.status
            ));
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

fn agent() -> Agent {
    let pem = std::env::var_os("SSL_CERT_FILE").and_then(|p| std::fs::read(p).ok());
    let certs = parse_pem(pem.as_deref().unwrap_or_default()).filter_map(|item| match item {
        Ok(PemItem::Certificate(c)) => Some(c),
        _ => None,
    });
    let certs: Vec<_> = certs.collect();
    let roots = (!certs.is_empty()).then(|| RootCerts::new_with_certs(&certs));
    let tls = TlsConfig::builder().root_certs(roots.unwrap_or(RootCerts::WebPki));
    let config = Agent::config_builder().http_status_as_error(false);
    config.tls_config(tls.build()).build().into()
}
