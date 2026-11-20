use crate::config::{Metric, RuleCheck};
use crate::metrics::Sample;
use crate::status::Status;

/// A rule's status and how many samples in a row read on the other side of its threshold.
#[derive(Default)]
pub struct Streak {
    down: bool,
    n: u32,
}

impl Streak {
    /// One sample, `over` its threshold or not; flips once the other side has held for `for_min` minutes.
    pub fn step(&mut self, over: bool, for_min: u16) -> Status {
        if over == self.down {
            self.n = 0;
        } else {
            self.n += 1;
            if self.n > u32::from(for_min) {
                (self.down, self.n) = (over, 0);
            }
        }
        if self.down { Status::Down } else { Status::Up }
    }
}

/// The rule's reading of `s` (disk or memory % used, load1 per core) with its message.
pub fn judge(r: &RuleCheck, s: &Sample, cores: f64, streak: &mut Streak) -> (Status, String) {
    let pct = |used: u64, total: u64| {
        let p = used.saturating_mul(100) / total.max(1);
        u32::try_from(p).ok().map(f64::from)
    };
    let (what, value, unit) = match (r.rule.metric, &r.mount) {
        (Metric::Disk, Some(m)) => {
            let d = s.disks.get(m);
            let v = d.and_then(|d| pct(d.used, d.used.saturating_add(d.free)));
            (format!("disk {}", m.as_str()), v, "% used")
        }
        (Metric::Memory, _) => {
            let v = s.mem.and_then(|g| pct(g.used, g.total));
            ("memory".into(), v, "% used")
        }
        (Metric::Load, _) | (Metric::Disk, None) => {
            let v = s.load.map(|l| f64::from(l[0]) / cores);
            ("load".into(), v, " per core")
        }
    };
    let (above, for_min) = (r.rule.above, r.rule.for_min);
    let Some(v) = value else {
        let st = streak.step(streak.down, for_min);
        return (st, format!("{what}: no reading"));
    };
    let st = streak.step(v > above, for_min);
    let msg = format!("{what} {v:.1}{unit} (down above {above} for {for_min} min)");
    (st, msg)
}
