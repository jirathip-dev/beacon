use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
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
    #[serde(default)]
    pub notify: NotifyPolicy,
}

/// The top-level `notify`: how often and how many times serve reminds that something is still down.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NotifyPolicy {
    #[serde(default = "four")]
    pub remind_every_h: u16,
    #[serde(default = "three")]
    pub remind_max: u8,
}

impl Default for NotifyPolicy {
    fn default() -> NotifyPolicy {
        NotifyPolicy {
            remind_every_h: four(),
            remind_max: three(),
        }
    }
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
    #[serde(default = "root")]
    pub disks: Vec<MountPath>,
    #[serde(default = "default_rules")]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub jobs: BTreeMap<Label, Review>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: Id,
    pub metric: Metric,
    pub above: f64,
    pub for_min: u16,
}

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    Disk,
    Memory,
    Load,
}

impl Host {
    /// Each rule as the checks it acts as: one per mount in `disks` for a disk rule, else one.
    pub fn rule_checks(&self) -> Result<Vec<RuleCheck>, String> {
        let mut out = Vec::new();
        for rule in &self.rules {
            let mounts = match rule.metric {
                Metric::Disk => self.disks.iter().map(Some).collect(),
                Metric::Memory | Metric::Load => vec![None],
            };
            for mount in mounts {
                let slug: String = mount
                    .map_or_default(|m| m.0.trim_matches('/').to_ascii_lowercase())
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect();
                let id = match slug.as_str() {
                    "" => rule.id.0.clone(),
                    slug => format!("{}-{slug}", rule.id.0),
                };
                out.push(RuleCheck {
                    id: Id::try_from(id)?,
                    rule: rule.clone(),
                    mount: mount.cloned(),
                });
            }
        }
        Ok(out)
    }
}

/// One status a rule keeps: the rule, and its mount for a disk rule.
#[derive(Debug, Clone)]
pub struct RuleCheck {
    pub id: Id,
    pub rule: Rule,
    pub mount: Option<MountPath>,
}

/// A launchd label or systemd unit name: 1 to 200 chars, no whitespace or control char.
#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[serde(try_from = "String")]
pub struct Label(String);

impl Label {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Label {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Label {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let bad = |c: char| c.is_whitespace() || c.is_control();
        let ok = (1..=200).contains(&s.chars().count()) && !s.contains(bad);
        ok.then(|| Label(s.clone()))
            .ok_or_else(|| format!("invalid job label {s:?}"))
    }
}

/// The owner's review of one job (ADR D12 §1).
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Review {
    Keep(Kept),
    Ignore,
    Removed,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    pub name: String,
    pub group: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub running: bool,
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

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[serde(try_from = "String")]
pub struct MountPath(String);

impl MountPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MountPath {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let ok = s.starts_with('/') && s.len() <= 255 && !s.contains(char::is_control);
        ok.then(|| MountPath(s.clone()))
            .ok_or_else(|| format!("invalid disk {s:?} (want an absolute path)"))
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
        let n = self.notify;
        range("notify.remind_every_h", n.remind_every_h.into(), 1, 168)?;
        range("notify.remind_max", n.remind_max.into(), 0, 100)?;
        for (host_id, host) in &self.hosts {
            let ctx = |e: String| format!("host {:?}: {e}", host_id.0);
            range("timeout_s", host.timeout_s.into(), 1, 30).map_err(ctx)?;
            let disks: BTreeSet<_> = host.disks.iter().collect();
            if host.disks.len() > 8 || disks.len() < host.disks.len() {
                return Err(ctx("disks must be at most 8 distinct paths".into()));
            }
            for (label, review) in &host.jobs {
                if let Review::Keep(k) = review {
                    let ctx = |e: String| format!("host {:?}: job {:?}: {e}", host_id.0, label.0);
                    text("name", &k.name, 60).map_err(ctx)?;
                    text("group", &k.group, 40).map_err(ctx)?;
                }
            }
            let mut seen = BTreeSet::new();
            for c in &host.checks {
                let ctx = |e: String| format!("host {:?}: check {:?}: {e}", host_id.0, c.id.0);
                if !seen.insert(&c.id) {
                    return Err(ctx("duplicate id".into()));
                }
                if c.id.0.starts_with("job-") {
                    return Err(ctx("ids starting with job- are reserved for jobs".into()));
                }
                text("command", &c.command, usize::MAX).map_err(ctx)?;
                range("every_s", c.every_s.into(), 10, 86400).map_err(ctx)?;
                range("timeout_s", c.timeout_s.into(), 1, 300).map_err(ctx)?;
                if u32::from(c.timeout_s) >= c.every_s {
                    return Err(ctx("timeout_s must be less than every_s".into()));
                }
            }
            let rules: BTreeSet<_> = host.rules.iter().map(|r| &r.id).collect();
            if rules.len() < host.rules.len() {
                return Err(ctx("duplicate rule id".into()));
            }
            for r in &host.rules {
                let ctx = |e: String| format!("host {:?}: rule {:?}: {e}", host_id.0, r.id.0);
                range("for_min", r.for_min.into(), 1, 1440).map_err(ctx)?;
                let most = if r.metric == Metric::Load {
                    1000.0
                } else {
                    100.0
                };
                if !(0.0..=most).contains(&r.above) {
                    return Err(ctx(format!("above {} out of 0..={most}", r.above)));
                }
            }
            let rule_checks = host.rule_checks().map_err(ctx)?;
            for r in &rule_checks {
                if !seen.insert(&r.id) {
                    return Err(ctx(format!("rule check {:?} reuses an id", r.id.0)));
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
                    let has_check = self.hosts.get(&b.host).is_some_and(|h| {
                        let rules = h.rule_checks().unwrap_or_default();
                        h.checks.iter().any(|c| c.id == b.id) || rules.iter().any(|r| r.id == b.id)
                    });
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

fn root() -> Vec<MountPath> {
    vec![MountPath("/".into())]
}
fn default_rules() -> Vec<Rule> {
    let rule = |id: &str, metric, above, for_min| Rule {
        id: Id(id.into()),
        metric,
        above,
        for_min,
    };
    vec![
        rule("disk-full", Metric::Disk, 90.0, 10),
        rule("memory-full", Metric::Memory, 90.0, 10),
        rule("load-high", Metric::Load, 2.0, 15),
    ]
}
fn three() -> u8 {
    3
}
fn four() -> u16 {
    4
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
