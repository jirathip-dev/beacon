use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Parses monitors.json v1 and enforces the schema's bounds and the cross-field rules.
pub fn parse(text: &str) -> Result<MonitorsConfig, String> {
    let config: MonitorsConfig = serde_json::from_str(text).map_err(|e| e.to_string())?;
    config.validate()?;
    Ok(config)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorsConfig {
    pub version: Version1,
    pub monitors: Vec<Monitor>,
    #[serde(default)]
    pub hosts: BTreeMap<Id, Host>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Monitor {
    Http(HttpMonitor),
    Tcp(TcpMonitor),
    Heartbeat(HeartbeatMonitor),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpMonitor {
    pub name: String,
    pub group: String,
    pub url: HttpsUrl,
    pub expect_status: Option<u16>,
    pub keyword: Option<String>,
    #[serde(default = "ten")]
    pub timeout_s: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpMonitor {
    pub name: String,
    pub group: String,
    pub host: String,
    pub port: u16,
    #[serde(default = "ten")]
    pub timeout_s: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatMonitor {
    pub name: String,
    pub group: String,
    pub host: Id,
    pub id: Id,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub url: HttpsUrl,
    #[serde(default = "five")]
    pub timeout_s: u8,
    pub checks: Vec<HostCheck>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct HostCheck {
    pub id: Id,
    pub command: String,
    #[serde(default = "sixty")]
    pub every_s: u32,
    #[serde(default = "thirty")]
    pub timeout_s: u16,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[serde(try_from = "String")]
pub struct Id(String);

impl Id {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Id {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let b = s.as_bytes();
        let ok = (1..=64).contains(&b.len())
            && b.iter().enumerate().all(|(i, c)| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && *c == b'-')
            });
        ok.then(|| Id(s.clone()))
            .ok_or_else(|| format!("invalid id {s:?} (want [a-z0-9][a-z0-9-]{{0,63}})"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct HttpsUrl(pub String);

impl TryFrom<String> for HttpsUrl {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        match s.strip_prefix("https://") {
            Some(rest) if !rest.is_empty() && !rest.contains(char::is_whitespace) => {
                Ok(HttpsUrl(s))
            }
            _ => Err(format!("invalid url {s:?} (want https://…)")),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "u64")]
pub struct Version1;

impl TryFrom<u64> for Version1 {
    type Error = String;
    fn try_from(v: u64) -> Result<Self, String> {
        (v == 1)
            .then_some(Version1)
            .ok_or_else(|| format!("unsupported version {v}"))
    }
}

impl MonitorsConfig {
    fn validate(&self) -> Result<(), String> {
        for (host_id, host) in &self.hosts {
            let ctx = |e: String| format!("host {:?}: {e}", host_id.0);
            range("timeout_s", host.timeout_s.into(), 1, 30).map_err(ctx)?;
            let mut seen = BTreeSet::new();
            for c in &host.checks {
                let ctx = |e: String| format!("host {:?}: check {:?}: {e}", host_id.0, c.id.0);
                if !seen.insert(&c.id) {
                    return Err(ctx("duplicate id".into()));
                }
                text("command", &c.command, usize::MAX).map_err(ctx)?;
                range("every_s", c.every_s.into(), 10, 86400).map_err(ctx)?;
                range("timeout_s", c.timeout_s.into(), 1, 300).map_err(ctx)?;
                if u32::from(c.timeout_s) >= c.every_s {
                    return Err(ctx("timeout_s must be less than every_s".into()));
                }
            }
        }
        for m in &self.monitors {
            let (name, group) = match m {
                Monitor::Http(h) => (&h.name, &h.group),
                Monitor::Tcp(t) => (&t.name, &t.group),
                Monitor::Heartbeat(b) => (&b.name, &b.group),
            };
            let ctx = |e: String| format!("monitor {name:?}: {e}");
            text("name", name, 60).map_err(ctx)?;
            text("group", group, 40).map_err(ctx)?;
            match m {
                Monitor::Http(h) => {
                    if let Some(s) = h.expect_status {
                        range("expect_status", s.into(), 100, 599).map_err(ctx)?;
                    }
                    if let Some(k) = &h.keyword {
                        text("keyword", k, usize::MAX).map_err(ctx)?;
                    }
                    range("timeout_s", h.timeout_s.into(), 1, 30).map_err(ctx)?;
                }
                Monitor::Tcp(t) => {
                    text("host", &t.host, usize::MAX).map_err(ctx)?;
                    range("port", t.port.into(), 1, 65535).map_err(ctx)?;
                    range("timeout_s", t.timeout_s.into(), 1, 30).map_err(ctx)?;
                }
                Monitor::Heartbeat(b) => {
                    let has_check = self
                        .hosts
                        .get(&b.host)
                        .is_some_and(|h| h.checks.iter().any(|c| c.id == b.id));
                    if !has_check {
                        return Err(format!(
                            "heartbeat {name:?}: host {:?} has no check {:?}",
                            b.host.0, b.id.0
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn text(field: &str, s: &str, max: usize) -> Result<(), String> {
    let n = s.chars().count();
    (1..=max)
        .contains(&n)
        .then_some(())
        .ok_or_else(|| format!("{field} must be 1..={max} chars"))
}

fn range(field: &str, v: u64, lo: u64, hi: u64) -> Result<(), String> {
    (lo..=hi)
        .contains(&v)
        .then_some(())
        .ok_or_else(|| format!("{field} {v} out of {lo}..={hi}"))
}

fn five() -> u8 {
    5
}
fn ten() -> u8 {
    10
}
fn sixty() -> u32 {
    60
}
fn thirty() -> u16 {
    30
}
