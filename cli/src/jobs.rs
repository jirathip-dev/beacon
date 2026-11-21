pub mod edit;
pub mod launchd;
pub mod systemd;

use crate::config::{Kept, Label, Review};
use crate::serve::every;
use crate::status::Status;
use crate::{out, probe};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub const JOBS_EVERY_S: u64 = 60;
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    KeepAlive,
    Scheduled(u64),
    OnDemand,
}

/// One line of the listing; `failed` is how its last run failed ("last exit 1"), none when clean.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub label: String,
    pub running: bool,
    pub failed: Option<String>,
}

/// One read of a job; `runs` is only compared.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub kind: Kind,
    pub runs: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub name: String,
    pub group: Option<String>,
    pub status: Status,
    pub message: String,
}

/// The down rule of ADR D12 §4; `must_run` is a kept job's `"running": true`.
pub fn judge(kind: &Kind, job: &Listed, missed: bool, must_run: bool) -> (Status, String) {
    let failed = job.failed.as_deref();
    let down = match kind {
        _ if must_run => !job.running,
        Kind::KeepAlive => !job.running,
        Kind::Scheduled(_) => failed.is_some() || missed,
        Kind::OnDemand => !job.running && failed.is_some(),
    };
    let message = match (kind, failed) {
        (Kind::Scheduled(_), Some(f)) => f.into(),
        (Kind::Scheduled(p), None) if missed => format!("no run in 2 intervals of {p}s"),
        _ if job.running => "running".into(),
        (_, Some(f)) if down => format!("not running, {f}"),
        _ if down => "not running".into(),
        (_, f) => f.unwrap_or("last exit 0").into(),
    };
    (if down { Status::Down } else { Status::Up }, message)
}

struct Seen {
    line: Listed,
    detail: Option<Detail>,
    at: Instant,
    idle: u8,
}

/// What discovery remembers between cycles: each listed job's last line, read and idle reads.
#[derive(Default)]
pub struct Watcher {
    seen: BTreeMap<String, Seen>,
}

impl Watcher {
    /// Reads the jobs that are new, changed or whose interval is up, then judges every listed and kept job.
    pub fn update(
        &mut self,
        listing: &[Listed],
        reviews: &BTreeMap<Label, Review>,
        now: Instant,
        read: impl Fn(&str) -> Option<Detail>,
    ) -> Vec<Row> {
        self.seen
            .retain(|l, _| listing.iter().any(|j| j.label == *l));
        for job in listing {
            let old = self.seen.get(&job.label);
            let up = |s: &Seen| match s.detail.as_ref().map(|d| &d.kind) {
                Some(Kind::Scheduled(p)) => now.saturating_duration_since(s.at) >= every(*p),
                _ => false,
            };
            let interval = old.is_some_and(up);
            if old.is_some_and(|s| s.line == *job) && !interval {
                continue;
            }
            let detail = read(&job.label);
            let runs = |d: &Option<Detail>| d.as_ref().map(|d| d.runs.clone());
            let (idle, at) = match old {
                Some(s) if runs(&s.detail) == runs(&detail) => (s.idle + u8::from(interval), s.at),
                Some(s) if !interval => (0, s.at),
                _ => (0, now),
            };
            let (line, idle, at) = (job.clone(), idle.min(2), if interval { now } else { at });
            self.seen.insert(
                job.label.clone(),
                Seen {
                    line,
                    detail,
                    at,
                    idle,
                },
            );
        }
        let mut rows = Vec::new();
        for job in listing {
            let kept = match reviews.get(job.label.as_str()) {
                Some(Review::Ignore) => continue,
                Some(Review::Keep(k)) => Some(k),
                _ => None,
            };
            let seen = self.seen.get(&job.label);
            let kind = seen
                .and_then(|s| s.detail.as_ref())
                .map_or(&Kind::OnDemand, |d| &d.kind);
            let missed = seen.is_some_and(|s| s.idle >= 2);
            let (status, message) = judge(kind, job, missed, kept.is_some_and(|k| k.running));
            rows.push(row(&job.label, kept, status, message));
        }
        for (label, review) in reviews {
            if let Review::Keep(k) = review
                && !listing.iter().any(|j| j.label == label.as_str())
            {
                let message = "not loaded".into();
                rows.push(row(label.as_str(), Some(k), Status::Down, message));
            }
        }
        let slugs: Vec<String> = rows.iter().map(|r| slug(&r.label)).collect();
        for (r, s) in rows.iter_mut().zip(&slugs) {
            let shared = s.len() > 60 || slugs.iter().filter(|o| *o == s).count() > 1;
            r.id = if shared {
                let hash = Sha256::digest(r.label.as_bytes());
                let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
                format!("job-{}-{hex}", s.chars().take(51).collect::<String>())
            } else {
                format!("job-{s}")
            };
        }
        rows
    }
}

fn row(label: &str, kept: Option<&Kept>, status: Status, message: String) -> Row {
    Row {
        id: String::new(),
        label: label.into(),
        name: kept.map_or(label.into(), |k| k.name.clone()),
        group: kept.map(|k| k.group.clone()),
        status,
        message,
    }
}

fn slug(label: &str) -> String {
    let keep = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let lower = label.chars().map(|c| c.to_ascii_lowercase());
    lower.map(|c| if keep(c) { c } else { '-' }).collect()
}

/// The user's job manager: launchd `gui/<uid>` or `systemd --user`.
pub enum Manager {
    Launchd { tool: String, uid: String },
    Systemd { tool: String, units: PathBuf },
}

impl Manager {
    /// `$BEACON_LAUNCHCTL` or `$BEACON_SYSTEMCTL` names the tool and picks the manager; else the OS does.
    pub fn here() -> Manager {
        let var = |k| std::env::var(k).ok();
        match (var("BEACON_LAUNCHCTL"), var("BEACON_SYSTEMCTL")) {
            (Some(tool), _) => Manager::launchd(tool),
            (None, Some(tool)) => Manager::systemd(tool),
            _ if cfg!(target_os = "macos") => Manager::launchd("launchctl".into()),
            _ => Manager::systemd("systemctl".into()),
        }
    }

    fn launchd(tool: String) -> Manager {
        let uid = probe::output("id", &["-u"], TOOL_TIMEOUT).unwrap_or_default();
        let uid = uid.trim().into();
        Manager::Launchd { tool, uid }
    }

    fn systemd(tool: String) -> Manager {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let units = PathBuf::from(home).join(".config/systemd/user");
        Manager::Systemd { tool, units }
    }

    /// One listing of the user's jobs, without Apple's, the distribution's and beacon's own.
    pub fn list(&self) -> Option<Vec<Listed>> {
        match self {
            Manager::Launchd { tool, .. } => {
                probe::output(tool, &["list"], TOOL_TIMEOUT).map(|t| launchd::listing(&t))
            }
            Manager::Systemd { tool, units } => {
                let args = ["--user", "list-units", "--type=service,timer", "--all"];
                let text = probe::output(
                    tool,
                    &[&args[..], &["--plain", "--no-legend"]].concat(),
                    TOOL_TIMEOUT,
                )?;
                let files = std::fs::read_dir(units).into_iter().flatten().flatten();
                let files: Vec<_> = files
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                Some(systemd::listing(&text, &files))
            }
        }
    }

    pub fn read(&self, label: &str) -> Option<Detail> {
        match self {
            Manager::Launchd { tool, uid } => {
                let target = format!("gui/{uid}/{label}");
                launchd::detail(&probe::output(tool, &["print", &target], TOOL_TIMEOUT)?)
            }
            Manager::Systemd { tool, .. } => {
                let show = |unit: &str, props: &str| {
                    let args = ["--user", "show", unit, "--timestamp=unix", "-p", props];
                    probe::output(tool, &args, TOOL_TIMEOUT)
                };
                let service = systemd::service_of(label);
                let timer = if label.strip_suffix(".timer").is_some() {
                    Some(show(label, systemd::TIMER_PROPS)?)
                } else {
                    None
                };
                systemd::detail(&show(&service, systemd::SERVICE_PROPS)?, timer.as_deref())
            }
        }
    }
}

/// Serve's discovery loop: one listing a cycle, reads when due, rows handed to `publish`.
pub fn watch(
    manager: &Manager,
    reviews: impl Fn() -> BTreeMap<Label, Review>,
    mut publish: impl FnMut(Vec<Row>),
) {
    let (mut watcher, mut failing) = (Watcher::default(), false);
    loop {
        match manager.list() {
            Some(listing) => {
                failing = false;
                let read = |label: &str| manager.read(label);
                publish(watcher.update(&listing, &reviews(), Instant::now(), read));
            }
            None if !failing => {
                failing = true;
                out::err("jobs: listing failed; keeping the last rows");
            }
            None => {}
        }
        thread::sleep(every(JOBS_EVERY_S));
    }
}
