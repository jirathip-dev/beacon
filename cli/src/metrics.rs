use crate::config::MountPath;
use crate::status::Rendered;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

pub const CAPACITY: usize = 1440;
pub const EVERY_S: u64 = 60;

/// One reading of the host; `None` is a read that failed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub t: u64,
    pub load: Option<[f32; 3]>,
    pub cpu_pct: Option<f32>,
    pub mem: Option<Gauge>,
    pub swap: Option<Gauge>,
    pub disks: BTreeMap<MountPath, DiskUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gauge {
    pub total: u64,
    pub used: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskUsage {
    pub used: u64,
    pub free: u64,
}

/// The last `CAPACITY` samples, oldest first.
#[derive(Default)]
pub struct Ring(VecDeque<Sample>);

#[derive(Serialize)]
struct Body<'a> {
    version: u8,
    host: &'a str,
    every_s: u64,
    t: Vec<u64>,
    load1: Vec<Option<f64>>,
    load5: Vec<Option<f64>>,
    load15: Vec<Option<f64>>,
    cpu_pct: Vec<Option<f64>>,
    mem: GaugeBody,
    swap: GaugeBody,
    disks: BTreeMap<&'a str, DiskBody>,
}

#[derive(Serialize)]
struct GaugeBody {
    total_bytes: Option<u64>,
    used_bytes: Vec<Option<u64>>,
}

#[derive(Serialize)]
struct DiskBody {
    used_bytes: Vec<Option<u64>>,
    free_bytes: Vec<Option<u64>>,
}

impl Ring {
    pub fn new() -> Ring {
        Ring(VecDeque::with_capacity(CAPACITY))
    }

    pub fn push(&mut self, sample: Sample) {
        if self.0.len() == CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn render(&self, host: &str, disks: &[MountPath]) -> Rendered {
        let col = |f: &dyn Fn(&Sample) -> Option<u64>| self.0.iter().map(f).collect();
        let load = |i: usize| {
            self.0
                .iter()
                .map(|s| s.load.map(|l| round(l[i], 100.0)))
                .collect()
        };
        let gauge = |f: fn(&Sample) -> Option<Gauge>| GaugeBody {
            total_bytes: self.0.iter().rev().find_map(|s| f(s).map(|g| g.total)),
            used_bytes: col(&|s| f(s).map(|g| g.used)),
        };
        let disk = |m: &MountPath, f: fn(&DiskUsage) -> u64| col(&|s| s.disks.get(m).map(f));
        let body = Body {
            version: 1,
            host,
            every_s: EVERY_S,
            t: self.0.iter().map(|s| s.t).collect(),
            load1: load(0),
            load5: load(1),
            load15: load(2),
            cpu_pct: self
                .0
                .iter()
                .map(|s| s.cpu_pct.map(|c| round(c, 10.0)))
                .collect(),
            mem: gauge(|s| s.mem),
            swap: gauge(|s| s.swap),
            disks: disks
                .iter()
                .map(|m| {
                    let used_bytes = disk(m, |d| d.used);
                    let free_bytes = disk(m, |d| d.free);
                    (
                        m.as_str(),
                        DiskBody {
                            used_bytes,
                            free_bytes,
                        },
                    )
                })
                .collect(),
        };
        Rendered::of(serde_json::to_vec(&body).unwrap())
    }
}

fn round(x: f32, scale: f64) -> f64 {
    (f64::from(x) * scale).round() / scale
}
