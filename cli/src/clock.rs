use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

/// Seconds since the Unix epoch; 0 before it.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `secs` divided by `BEACON_SPEEDUP` (default 1), the period every loop sleeps by.
pub fn every(secs: u64) -> Duration {
    static SPEEDUP: OnceLock<u32> = OnceLock::new();
    let k = SPEEDUP.get_or_init(|| {
        let k = std::env::var("BEACON_SPEEDUP")
            .ok()
            .and_then(|k| k.parse().ok());
        k.filter(|&k| k > 0).unwrap_or(1)
    });
    Duration::from_secs(secs) / *k
}
