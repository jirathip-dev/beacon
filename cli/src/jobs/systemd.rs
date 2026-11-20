use super::{Detail, Kind, Listed};
use std::collections::BTreeMap;

pub const SERVICE_PROPS: &str = "Restart,InactiveExitTimestampMonotonic";
pub const TIMER_PROPS: &str =
    "TimersMonotonic,TimersCalendar,LastTriggerUSec,NextElapseUSecRealtime";

/// The service a unit's job runs: a timer's same-named service.
pub fn service_of(unit: &str) -> String {
    unit.strip_suffix(".timer")
        .map_or(unit.into(), |stem| format!("{stem}.service"))
}

/// Parses `list-units --plain --no-legend` for the unit `files` of `~/.config/systemd/user`; a timer and its service are one job.
pub fn listing(text: &str, files: &[String]) -> Vec<Listed> {
    let active: BTreeMap<&str, &str> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let unit = f.next()?;
            Some((unit, f.nth(1)?))
        })
        .collect();
    let mut jobs: Vec<_> = files
        .iter()
        .filter(|f| {
            f.strip_suffix(".timer")
                .or(f.strip_suffix(".service"))
                .is_some()
        })
        .filter(|f| *f != "beacon-serve.service" && active.contains_key(f.as_str()))
        .filter(|f| {
            let timer = f.strip_suffix(".service").map(|s| format!("{s}.timer"));
            !timer.is_some_and(|t| files.contains(&t))
        })
        .map(|f| {
            let state = active
                .get(service_of(f).as_str())
                .copied()
                .unwrap_or("inactive");
            Listed {
                label: f.clone(),
                running: state == "active",
                failed: (state == "failed").then(|| "failed".into()),
            }
        })
        .collect();
    jobs.sort_by(|a, b| a.label.cmp(&b.label));
    jobs
}

/// Reads the kind and run count from `systemctl --user show` of the service and, for a timer job, the timer.
pub fn detail(service: &str, timer: Option<&str>) -> Option<Detail> {
    let props = |text: &str| -> BTreeMap<String, String> {
        let pairs = text.lines().filter_map(|l| l.split_once('='));
        pairs.map(|(k, v)| (k.into(), v.into())).collect()
    };
    let svc = props(service);
    let runs = svc.get("InactiveExitTimestampMonotonic")?.clone();
    let kind = match timer.map(props) {
        Some(t) => Kind::Scheduled(period(&t).max(1)),
        None if svc.get("Restart").is_some_and(|r| r != "no") => Kind::KeepAlive,
        None => Kind::OnDemand,
    };
    Some(Detail { kind, runs })
}

fn period(timer: &BTreeMap<String, String>) -> u64 {
    let mono = timer.get("TimersMonotonic").and_then(|v| {
        let at = v
            .find("OnUnitActiveUSec=")
            .or(v.find("OnUnitInactiveUSec="))?;
        let span = v.get(at..)?.split_once('=')?.1;
        Some(seconds(span.split(" ;").next()?))
    });
    let stamp = |k: &str| timer.get(k)?.strip_prefix('@')?.parse::<u64>().ok();
    let calendar = stamp("NextElapseUSecRealtime").zip(stamp("LastTriggerUSec"));
    let calendar = calendar.map(|(next, last)| next.saturating_sub(last));
    mono.filter(|&s| s > 0).or(calendar).unwrap_or(86_400)
}

fn seconds(span: &str) -> u64 {
    let units = [
        ("min", 60),
        ("ms", 0),
        ("us", 0),
        ("s", 1),
        ("h", 3600),
        ("d", 86_400),
        ("w", 604_800),
    ];
    span.split_whitespace()
        .filter_map(|t| {
            let split = t.find(|c: char| !c.is_ascii_digit())?;
            let (n, unit) = t.split_at(split);
            let k = units.iter().find(|(u, _)| *u == unit)?.1;
            n.parse::<u64>().ok().map(|n| n * k)
        })
        .sum()
}
