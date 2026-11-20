use crate::config::MountPath;
use crate::metrics::{DiskUsage, Gauge, Sample};
use crate::probe;
use std::collections::BTreeMap;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Cumulative cpu ticks since boot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuTicks {
    pub busy: u64,
    pub total: u64,
}

/// Reads this host; keeps the previous cpu ticks so `cpu_pct` is the mean since the last sample.
#[derive(Default)]
pub struct Sampler {
    prev: Option<CpuTicks>,
}

impl Sampler {
    pub fn new() -> Sampler {
        Sampler::default()
    }

    pub fn sample(&mut self, t: u64, disks: &[MountPath]) -> Sample {
        let ticks = os::cpu_ticks();
        let cpu_pct = self.prev.zip(ticks).and_then(|(p, n)| cpu_pct(p, n));
        self.prev = ticks;
        let (load, mem, swap) = os::load_mem_swap();
        let disks: BTreeMap<_, _> = disks
            .iter()
            .filter_map(|m| {
                let out = probe::output("/bin/df", &["-kP", "--", m.as_str()], TIMEOUT)?;
                Some((m.clone(), df(&out)?))
            })
            .collect();
        Sample {
            t,
            load,
            cpu_pct,
            mem,
            swap,
            disks,
        }
    }
}

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "tick counts and a 0..=100 percentage fit f64 and f32 well inside the 0.1 /metrics rounds to"
)]
pub fn cpu_pct(prev: CpuTicks, now: CpuTicks) -> Option<f32> {
    let busy = now.busy.checked_sub(prev.busy)?;
    let total = now.total.checked_sub(prev.total).filter(|t| *t > 0)?;
    Some((100.0 * busy as f64 / total as f64) as f32)
}

pub fn linux_load(loadavg: &str) -> Option<[f32; 3]> {
    let mut f = loadavg.split_whitespace().map(|x| x.parse().ok());
    Some([f.next()??, f.next()??, f.next()??])
}

pub fn linux_cpu(stat: &str) -> Option<CpuTicks> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let f: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let total: u64 = f.iter().sum();
    let [_, _, _, idle, iowait, _, _, _] = <[u64; 8]>::try_from(f).ok()?;
    Some(CpuTicks {
        busy: total - idle - iowait,
        total,
    })
}

pub fn linux_mem(meminfo: &str) -> Option<(Gauge, Gauge)> {
    let kb = |key: &str| -> Option<u64> {
        let line = meminfo
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?;
        Some(line.split_whitespace().next()?.parse::<u64>().ok()? * 1024)
    };
    let gauge = |total: u64, free: u64| Gauge {
        total,
        used: total.saturating_sub(free),
    };
    let mem = gauge(kb("MemTotal")?, kb("MemAvailable")?);
    Some((mem, gauge(kb("SwapTotal")?, kb("SwapFree")?)))
}

pub fn macos_load(sysctl: &str) -> Option<[f32; 3]> {
    let line = sysctl
        .lines()
        .next()?
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?;
    linux_load(line)
}

pub fn macos_mem(vm_stat: &str, memsize: &str) -> Option<Gauge> {
    let page: u64 = vm_stat
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |key: &str| -> Option<u64> {
        let line = vm_stat
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?;
        line.trim().trim_end_matches('.').parse().ok()
    };
    let used = (pages("Anonymous pages")?
        + pages("Pages wired down")?
        + pages("Pages occupied by compressor")?)
    .saturating_sub(pages("Pages purgeable")?);
    Some(Gauge {
        total: memsize.trim().parse().ok()?,
        used: used * page,
    })
}

pub fn macos_swap(sysctl: &str) -> Option<Gauge> {
    let line = sysctl.lines().nth(2)?;
    let size = |key: &str| -> Option<u64> {
        let v = line.split(key).nth(1)?.split_whitespace().next()?;
        let (n, unit) = v.split_at(v.len().checked_sub(1)?);
        let shift = 10 * " KMGT".find(unit)?;
        let (whole, frac) = n.split_once('.').unwrap_or((n, ""));
        let scale = 10u64.checked_pow(u32::try_from(frac.len()).ok()?)?;
        let frac = if frac.is_empty() {
            0
        } else {
            frac.parse::<u64>().ok()?
        };
        let units = whole.parse::<u64>().ok()?.checked_mul(scale)? + frac;
        Some(units.checked_mul(1 << shift)? / scale)
    };
    Some(Gauge {
        total: size("total = ")?,
        used: size("used = ")?,
    })
}

pub fn df(output: &str) -> Option<DiskUsage> {
    let f: Vec<&str> = output.lines().last()?.split_whitespace().collect();
    f.windows(4).find_map(|w| {
        let [blocks, used, avail, pct] = w else {
            return None;
        };
        pct.strip_suffix('%')?.parse::<u8>().ok()?;
        used.parse::<u64>().ok()?;
        let (blocks, avail) = (blocks.parse::<u64>().ok()?, avail.parse::<u64>().ok()?);
        Some(DiskUsage {
            used: blocks.saturating_sub(avail) * 1024,
            free: avail * 1024,
        })
    })
}

#[cfg(target_os = "linux")]
mod os {
    use super::{CpuTicks, Gauge, linux_cpu, linux_load, linux_mem};

    fn read(path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    pub fn cpu_ticks() -> Option<CpuTicks> {
        linux_cpu(&read("/proc/stat")?)
    }

    pub fn load_mem_swap() -> (Option<[f32; 3]>, Option<Gauge>, Option<Gauge>) {
        let mem = read("/proc/meminfo").as_deref().and_then(linux_mem);
        let load = read("/proc/loadavg").as_deref().and_then(linux_load);
        (load, mem.map(|m| m.0), mem.map(|m| m.1))
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::{CpuTicks, Gauge, TIMEOUT, macos_load, macos_mem, macos_swap, probe};

    pub fn cpu_ticks() -> Option<CpuTicks> {
        let [user, system, idle, nice] = beacon_sys::host_cpu_load()?.map(u64::from);
        Some(CpuTicks {
            busy: user + system + nice,
            total: user + system + idle + nice,
        })
    }

    pub fn load_mem_swap() -> (Option<[f32; 3]>, Option<Gauge>, Option<Gauge>) {
        let args = ["-n", "vm.loadavg", "hw.memsize", "vm.swapusage"];
        let Some(sysctl) = probe::output("/usr/sbin/sysctl", &args, TIMEOUT) else {
            return (None, None, None);
        };
        let memsize = sysctl.lines().nth(1).unwrap_or("");
        let mem =
            probe::output("/usr/bin/vm_stat", &[], TIMEOUT).and_then(|v| macos_mem(&v, memsize));
        (macos_load(&sysctl), mem, macos_swap(&sysctl))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod os {
    use super::{CpuTicks, Gauge};

    pub fn cpu_ticks() -> Option<CpuTicks> {
        None
    }

    pub fn load_mem_swap() -> (Option<[f32; 3]>, Option<Gauge>, Option<Gauge>) {
        (None, None, None)
    }
}
