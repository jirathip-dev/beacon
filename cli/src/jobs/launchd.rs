use super::{Detail, Kind, Listed};

const LEFT_OUT: [&str; 2] = ["com.apple.", "application."];

/// Parses `launchctl list`: `PID Status Label` lines, a negative status a signal.
pub fn listing(text: &str) -> Vec<Listed> {
    let jobs = text.lines().skip(1).filter_map(|line| {
        let mut f = line.split('\t');
        let (pid, status, label) = (f.next()?, f.next()?, f.next()?);
        let ours = !LEFT_OUT.iter().any(|p| label.starts_with(p)) && label != "dev.beacon.serve";
        let failed = match status.strip_prefix('-') {
            _ if status == "0" || status == "-" => None,
            Some(signal) => Some(format!("last exit signal {signal}")),
            None => Some(format!("last exit {status}")),
        };
        let running = pid != "-";
        ours.then(|| Listed {
            label: label.into(),
            running,
            failed,
        })
    });
    jobs.collect()
}

/// Reads the kind and the run count from `launchctl print gui/<uid>/<label>`.
pub fn detail(text: &str) -> Option<Detail> {
    let field = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key));
    let runs = field("runs = ")?.into();
    let keepalive =
        field("properties = ").is_some_and(|p| p.split(" | ").any(|p| p == "keepalive"));
    let interval = field("run interval = ").and_then(|v| v.strip_suffix(" seconds")?.parse().ok());
    let calendar = text
        .contains("stream = com.apple.launchd.calendarinterval")
        .then(|| {
            let spans = [
                ("\"Month\"", 366),
                ("\"Day\"", 31),
                ("\"Weekday\"", 7),
                ("\"Hour\"", 1),
            ];
            let days = spans
                .iter()
                .find(|(k, _)| text.contains(k))
                .map(|(_, d)| d * 86_400);
            days.unwrap_or(3600)
        });
    let kind = match interval.or(calendar) {
        _ if keepalive => Kind::KeepAlive,
        Some(p) => Kind::Scheduled(p),
        None => Kind::OnDemand,
    };
    Some(Detail { kind, runs })
}
