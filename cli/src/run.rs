use crate::config::Host;
use crate::status::Status;
use crate::{out, probe};
use std::time::Duration;

/// Runs every check of `host` once, concurrently, and prints one line per check in config order.
/// Returns true when all are up.
pub fn round(host: &Host) -> bool {
    let results: Vec<_> = std::thread::scope(|s| {
        let runs: Vec<_> = host
            .checks
            .iter()
            .map(|c| {
                let timeout = Duration::from_secs(c.timeout_s.into());
                s.spawn(move || probe::run(&c.command, timeout))
            })
            .collect();
        runs.into_iter()
            .map(|r| {
                r.join()
                    .unwrap_or_else(|_| (Status::Down, "check thread panicked".into()))
            })
            .collect()
    });
    let width = host
        .checks
        .iter()
        .map(|c| c.id.as_str().len())
        .max()
        .unwrap_or(0);
    for (c, (status, message)) in host.checks.iter().zip(&results) {
        out::out(&format!(
            "{:width$}  {status:4}  {message:?}",
            c.id.as_str()
        ));
    }
    results.iter().all(|(s, _)| *s == Status::Up)
}
