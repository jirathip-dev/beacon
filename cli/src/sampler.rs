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
    (f.len() == 8).then(|| CpuTicks {
        busy: total - f[3] - f[4],
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
        Some((n.parse::<f64>().ok()? * (1u64 << shift) as f64) as u64)
    };
    Some(Gauge {
        total: size("total = ")?,
        used: size("used = ")?,
    })
}

pub fn df(output: &str) -> Option<DiskUsage> {
    let f: Vec<&str> = output.lines().last()?.split_whitespace().collect();
    f.windows(4).find_map(|w| {
        let n = |i: usize| w[i].parse::<u64>().ok();
        w[3].strip_suffix('%')?.parse::<u8>().ok()?;
        let (blocks, avail) = (n(0)?, n(2)?);
        n(1)?;
        Some(DiskUsage {
            used: blocks.saturating_sub(avail) * 1024,
            free: avail * 1024,
        })
    })
}

#[cfg(target_os = "linux")]
mod os {
    use super::*;

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
    use super::*;

    pub fn cpu_ticks() -> Option<CpuTicks> {
        unsafe extern "C" {
            fn mach_host_self() -> u32;
            fn host_statistics(host: u32, flavor: i32, info: *mut u32, count: *mut u32) -> i32;
        }
        static HOST: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        let (mut info, mut count) = ([0u32; 4], 4u32);
        // SAFETY: HOST_CPU_LOAD_INFO (3) fills at most `count` (4) u32s of `info`, both owned here.
        let ok = unsafe {
            host_statistics(
                *HOST.get_or_init(|| mach_host_self()),
                3,
                info.as_mut_ptr(),
                &mut count,
            )
        };
        let [user, system, idle, nice] = info.map(u64::from);
        (ok == 0).then_some(CpuTicks {
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
    use super::*;

    pub fn cpu_ticks() -> Option<CpuTicks> {
        None
    }

    pub fn load_mem_swap() -> (Option<[f32; 3]>, Option<Gauge>, Option<Gauge>) {
        (None, None, None)
    }
}
