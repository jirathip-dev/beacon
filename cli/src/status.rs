use crate::config::Id;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Up,
    Down,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(match self {
            Status::Up => "up",
            Status::Down => "down",
        })
    }
}

/// A response body as a request reads it: the bytes and their `ETag`.
#[derive(Clone)]
pub struct Rendered {
    pub body: Arc<[u8]>,
    pub etag: String,
}

impl Rendered {
    pub fn of(body: Vec<u8>) -> Rendered {
        let mut h = DefaultHasher::new();
        body.hash(&mut h);
        Rendered {
            body: body.into(),
            etag: format!("\"{:016x}\"", h.finish()),
        }
    }

    #[expect(
        clippy::unwrap_used,
        reason = "serde_json fails only on non-string map keys or a failing Serialize impl; beacon's bodies have neither"
    )]
    pub fn json(body: &impl Serialize) -> Rendered {
        Rendered::of(serde_json::to_vec(body).unwrap())
    }
}

#[derive(Serialize)]
pub struct StatusBody {
    version: u8,
    host: String,
    checks: BTreeMap<String, CheckResult>,
}

#[derive(Serialize, PartialEq)]
struct CheckResult {
    status: Status,
    message: String,
    since: String,
}

impl StatusBody {
    pub fn new(host: &str) -> StatusBody {
        StatusBody {
            version: 1,
            host: host.into(),
            checks: BTreeMap::new(),
        }
    }

    /// Records one run at Unix time `now`; true when status or message changed.
    pub fn record(&mut self, id: &Id, status: Status, message: String, now: u64) -> bool {
        let old = self.checks.get(id.as_str());
        let since = match old {
            Some(o) if o.status == status => o.since.clone(),
            _ => rfc3339(now),
        };
        let new = CheckResult {
            status,
            message,
            since,
        };
        let changed = old != Some(&new);
        self.checks.insert(id.as_str().into(), new);
        changed
    }

    /// Drops the results of checks not in `ids`.
    pub fn keep(&mut self, ids: &[&Id]) {
        self.checks
            .retain(|k, _| ids.iter().any(|id| id.as_str() == k));
    }

    pub fn render(&self) -> Rendered {
        Rendered::json(self)
    }
}

fn rfc3339(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or_default() + 719_468;
    let secs = unix % 86_400;
    let (era, doe) = (days.div_euclid(146_097), days.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, (mp + 2) % 12 + 1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (h, mi, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
