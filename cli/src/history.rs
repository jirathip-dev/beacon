use crate::config::Id;
use crate::serve::over_budget;
use crate::status::{Rendered, Status};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::ops::Range;

pub const INCIDENTS: usize = 64;
pub const KEEP_S: u64 = 30 * 86_400;
pub const MESSAGE: usize = 120;
const GAPS: usize = 128;
const GAP_S: u64 = 180;
const HOURLY: usize = 16 + 2 * 1440;
const RING_BYTES: usize = HOURLY + 4 * 720;
const NONE: u64 = 0xFFFF;
const NO_RESPONSE: u64 = 0xFFFE;

/// Every key's incidents and serve's gaps: the `/history` body and the bytes of `events.json`.
#[derive(Serialize, Deserialize, Default)]
pub struct History {
    version: u8,
    host: String,
    gaps: VecDeque<(u64, u64)>,
    checks: BTreeMap<String, Track>,
    monitors: Vec<MonitorTrack>,
    #[serde(skip)]
    alive: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Track {
    since: u64,
    down: VecDeque<Incident>,
}

#[derive(Serialize, Deserialize)]
struct Incident {
    down_at: u64,
    up_at: Option<u64>,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct MonitorTrack {
    id: MonitorId,
    group: String,
    name: String,
    #[serde(flatten)]
    track: Track,
}

/// 16 hex digits: the FNV-1a 64 hash of `<group>/<name>`.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct MonitorId(pub String);

impl MonitorId {
    pub fn of(group: &str, name: &str) -> MonitorId {
        let fnv = |h: u64, b: u8| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        let h = format!("{group}/{name}")
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325, fnv);
        MonitorId(format!("{h:016x}"))
    }
}

#[derive(Clone, Copy)]
pub enum Key<'a> {
    Check(&'a Id),
    Monitor(&'a MonitorId),
}

impl Track {
    fn trim(&mut self, old: u64) -> bool {
        let stale = |t: &Track| {
            let old = t
                .down
                .front()
                .is_some_and(|i| i.up_at.is_some_and(|u| u < old));
            old || t.down.len() > INCIDENTS && !over_budget("history")
        };
        let changed = stale(self);
        while stale(self) {
            let i = self.down.pop_front();
            self.since = i.map_or(self.since, |i| i.up_at.unwrap_or(i.down_at));
        }
        changed
    }
}

impl History {
    pub fn new(host: &str, alive: u64) -> History {
        let host = host.into();
        History {
            version: 1,
            host,
            alive,
            ..History::default()
        }
    }

    /// The history in `events`, last written at `alive`; `None` when unreadable or another version.
    pub fn load(host: &str, events: &[u8], alive: u64) -> Option<History> {
        let h: History = serde_json::from_slice(events).ok()?;
        (h.version == 1).then(|| History {
            host: host.into(),
            alive,
            ..h
        })
    }

    /// One result of `key` at `now`; true when it went down or up, or is new.
    pub fn record(&mut self, key: Key, status: Status, message: &str, now: u64) -> bool {
        let (new, track) = match key {
            Key::Check(id) => {
                let new = !self.checks.contains_key(id.as_str());
                let track = self.checks.entry(id.as_str().into());
                (
                    new,
                    track.or_insert(Track {
                        since: now,
                        ..Track::default()
                    }),
                )
            }
            Key::Monitor(id) => match self.monitors.iter_mut().find(|m| &m.id == id) {
                Some(m) => (false, &mut m.track),
                None => return false,
            },
        };
        match (status, track.down.back_mut().filter(|i| i.up_at.is_none())) {
            (Status::Down, None) => {
                let message = message.chars().take(MESSAGE).collect();
                track.down.push_back(Incident {
                    down_at: now,
                    up_at: None,
                    message,
                });
                track.trim(0);
                true
            }
            (Status::Up, Some(i)) => {
                i.up_at = Some(now);
                true
            }
            _ => new,
        }
    }

    /// Drops the checks not in `checks`; the monitors become `monitors`, in order, keeping their tracks.
    pub fn keep(&mut self, checks: &[&Id], monitors: &[(&MonitorId, &str, &str)], now: u64) {
        let ours = |k: &String| k.starts_with("job-") || checks.iter().any(|id| id.as_str() == k);
        self.checks.retain(|k, _| ours(k));
        let mut old = std::mem::take(&mut self.monitors);
        for &(id, group, name) in monitors {
            let at = old.iter().position(|m| m.id == *id);
            let track = at.map(|i| old.swap_remove(i).track);
            let track = track.unwrap_or(Track {
                since: now,
                ..Track::default()
            });
            let (id, group, name) = (id.clone(), group.into(), name.into());
            self.monitors.push(MonitorTrack {
                id,
                group,
                name,
                track,
            });
        }
    }

    /// Serve is alive at `now`: a gap when it was not for over `GAP_S`; drops what is past `KEEP_S`.
    pub fn tick(&mut self, now: u64) -> bool {
        let mut changed = self.alive > 0 && now.saturating_sub(self.alive) > GAP_S;
        if changed {
            self.gaps.push_back((self.alive, now));
        }
        let old = now.saturating_sub(KEEP_S);
        self.alive = now;
        while self.gaps.len() > GAPS || self.gaps.front().is_some_and(|g| g.1 < old) {
            changed |= self.gaps.pop_front().is_some();
        }
        let monitors = self.monitors.iter_mut().map(|m| &mut m.track);
        for t in self.checks.values_mut().chain(monitors) {
            changed |= t.trim(old);
        }
        changed
    }

    pub fn render(&self) -> Rendered {
        Rendered::json(self)
    }
}

/// One monitor's response times: the image of its ring file, 24 h of probes and 30 d of hours.
pub struct Times(Box<[u8; RING_BYTES]>);

impl Times {
    /// The ring in `file`; empty when it has another size or header.
    pub fn load(file: Option<&[u8]>) -> Times {
        let mut ring = Times(Box::new([0xFF; RING_BYTES]));
        ring.set(0, 8, u64::from_le_bytes(*b"BCNT\x01\0\0\0"));
        ring.set(8, 8, 0);
        if let Some(f) = file.filter(|f| f.len() == RING_BYTES && f.get(..8) == ring.0.get(..8)) {
            ring.0.copy_from_slice(f);
        }
        ring
    }

    pub fn image(&self) -> &[u8] {
        self.0.as_slice()
    }

    fn at(&self, at: usize, n: usize) -> u64 {
        let le = |b: &[u8]| b.iter().rev().fold(0, |v, &b| v << 8 | u64::from(b));
        self.0.get(at..at + n).map_or(NONE, le)
    }

    fn set(&mut self, at: usize, n: usize, v: u64) {
        let v = v.to_le_bytes();
        if let Some((to, from)) = self.0.get_mut(at..at + n).zip(v.get(..n)) {
            to.copy_from_slice(from);
        }
    }

    /// Stores the probe of `minute` (Unix seconds ÷ 60); the byte ranges it changed.
    pub fn put(&mut self, minute: u64, ms: Option<u16>) -> [Range<usize>; 3] {
        let (last, hour) = (self.at(8, 8), minute / 60);
        let (m_at, h_at) = (minute_at(minute), hour_at(hour));
        let mut changed = [8..16, m_at..m_at + 2, h_at..h_at + 4];
        if last > 0 && minute > last + 1 {
            for m in (last + 1..minute).rev().take(1440) {
                self.set(minute_at(m), 2, NONE);
            }
            changed[1] = 16..HOURLY;
        }
        if last > 0 && hour > last / 60 + 1 {
            for h in (last / 60 + 1..hour).rev().take(720) {
                self.set(hour_at(h), 4, u64::MAX);
            }
            changed[2] = HOURLY..RING_BYTES;
        }
        self.set(
            m_at,
            2,
            ms.map_or(NO_RESPONSE, |ms| u64::from(ms).min(NO_RESPONSE - 1)),
        );
        self.set(8, 8, minute.max(last));
        let seen = (hour * 60..=minute).map(|m| self.at(minute_at(m), 2));
        let seen: Vec<_> = seen.filter(|&v| v < NO_RESPONSE).collect();
        let n = u64::try_from(seen.len()).unwrap_or(0);
        let avg = seen.iter().sum::<u64>().checked_div(n).unwrap_or(NONE);
        self.set(h_at, 4, avg | seen.iter().max().unwrap_or(&NONE) << 16);
        changed
    }

    /// The `/history/<id>` body.
    pub fn render(&self, id: &MonitorId) -> Rendered {
        let last = self.at(8, 8);
        let minutes = (last.saturating_sub(1439)..=last).filter(|_| last > 0);
        let recent: Vec<_> = minutes
            .map(|m| (m * 60, self.at(minute_at(m), 2)))
            .collect();
        let recent = recent.iter().filter(|r| r.1 != NONE);
        let ms = recent.clone().map(|r| (r.1 != NO_RESPONSE).then_some(r.1));
        let hours = ((last / 60).saturating_sub(719)..=last / 60).filter(|_| last > 0);
        let hourly: Vec<_> = hours
            .map(|h| (h * 3600, self.at(hour_at(h), 2), self.at(hour_at(h) + 2, 2)))
            .collect();
        let hourly = hourly.iter().filter(|h| h.1 != NONE);
        let json = |v: Vec<Option<u64>>| serde_json::to_string(&v).unwrap_or_default();
        let column =
            |f: fn(&(u64, u64, u64)) -> u64| json(hourly.clone().map(|h| Some(f(h))).collect());
        Rendered::of(format!(
            r#"{{"version":1,"id":"{}","every_s":60,"recent":{{"t":{},"ms":{}}},"hourly":{{"t":{},"avg_ms":{},"max_ms":{}}}}}"#,
            id.0,
            json(recent.map(|r| Some(r.0)).collect()),
            json(ms.collect()),
            column(|h| h.0),
            column(|h| h.1),
            column(|h| h.2),
        ).into_bytes())
    }
}

fn minute_at(minute: u64) -> usize {
    16 + 2 * usize::try_from(minute % 1440).unwrap_or(0)
}

fn hour_at(hour: u64) -> usize {
    HOURLY + 4 * usize::try_from(hour % 720).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Id {
        Id::try_from(s.to_string()).unwrap()
    }

    fn body(r: &Rendered) -> serde_json::Value {
        serde_json::from_slice(&r.body).unwrap()
    }

    #[test]
    fn render_gives_the_example_bytes() {
        let (backup, disk, blog) = (id("backup"), id("disk"), MonitorId::of("Web", "Blog"));
        let mut h = History::new("macmini", 0);
        h.keep(&[&backup, &disk], &[(&blog, "Web", "Blog")], 1_794_000_000);
        h.record(Key::Check(&backup), Status::Up, "ok", 1_794_000_000);
        h.record(Key::Check(&disk), Status::Up, "", 1_794_000_000);
        h.tick(1_794_700_000);
        assert!(h.tick(1_794_703_600));
        assert!(h.record(Key::Check(&backup), Status::Down, "exit 1", 1_794_787_200));
        assert!(!h.record(Key::Check(&backup), Status::Down, "exit 2", 1_794_787_260));
        assert!(h.record(Key::Check(&backup), Status::Up, "ok", 1_794_790_800));
        assert!(!h.record(Key::Check(&disk), Status::Up, "40%", 1_794_790_800));
        let cert = "cert expires in 13d";
        assert!(h.record(Key::Monitor(&blog), Status::Down, cert, 1_794_900_000));
        let example = std::fs::read("../docs/history.example.json").unwrap();
        assert_eq!(&h.render().body[..], &example[..]);
        let again = History::load("macmini", &example, 1_794_900_000).unwrap();
        assert_eq!(&again.render().body[..], &example[..]);

        let mut t = Times::load(None);
        t.put(29_914_998, Some(212));
        t.put(29_914_999, None);
        t.put(29_915_000, Some(198));
        let example = std::fs::read("../docs/times.example.json").unwrap();
        assert_eq!(&t.render(&blog).body[..], &example[..]);
        assert_eq!(blog.0, "db178b9377caae1a");
    }

    #[test]
    fn a_key_keeps_64_incidents_and_moves_since() {
        let c = id("flap");
        let mut h = History::new("h", 0);
        for i in 0..100 {
            h.record(
                Key::Check(&c),
                Status::Down,
                &"x".repeat(200),
                1000 + i * 10,
            );
            h.record(Key::Check(&c), Status::Up, "", 1005 + i * 10);
        }
        let track = &body(&h.render())["checks"]["flap"];
        assert_eq!(track["down"].as_array().unwrap().len(), INCIDENTS);
        assert_eq!(track["since"], 1000 + 35 * 10 + 5);
        assert_eq!(track["down"][0]["message"].as_str().unwrap().len(), MESSAGE);
    }

    #[test]
    fn tick_drops_incidents_and_gaps_past_30_days() {
        let c = id("c");
        let mut h = History::new("h", 0);
        h.record(Key::Check(&c), Status::Down, "", 1000);
        h.record(Key::Check(&c), Status::Up, "", 2000);
        h.record(Key::Check(&c), Status::Down, "", 3000);
        h.tick(5000);
        assert!(h.tick(9000));
        assert!(!h.tick(9060));
        assert!(h.tick(9060 + KEEP_S));
        let b = body(&h.render());
        assert_eq!(b["gaps"], serde_json::json!([[9060, 9060 + KEEP_S]]));
        assert_eq!(
            b["checks"]["c"]["down"],
            serde_json::json!([{ "down_at": 3000, "up_at": null, "message": "" }])
        );
        assert_eq!(b["checks"]["c"]["since"], 2000);
    }

    #[test]
    fn a_ring_holds_24_hours_and_30_days_in_5776_bytes() {
        let mut t = Times::load(None);
        for m in 0..2000 {
            t.put(30_000_000 + m, Some(100));
        }
        assert_eq!(t.image().len(), 5776);
        let b = body(&t.render(&MonitorId::of("g", "n")));
        assert_eq!(b["recent"]["t"].as_array().unwrap().len(), 1440);
        assert_eq!(b["hourly"]["t"].as_array().unwrap().len(), 34);
        let again = Times::load(Some(t.image()));
        assert_eq!(again.image(), t.image());
        assert_eq!(
            Times::load(Some(&[0; 10])).image(),
            Times::load(None).image()
        );
        let changed = t.put(30_002_000, Some(5));
        assert_eq!(
            changed.iter().map(ExactSizeIterator::len).sum::<usize>(),
            14
        );
        t.put(30_002_000 + 1445, Some(7));
        let b = body(&t.render(&MonitorId::of("g", "n")));
        assert_eq!(
            b["recent"]["t"],
            serde_json::json!([(30_002_000 + 1445) * 60])
        );
        assert_eq!(b["hourly"]["max_ms"].as_array().unwrap().last().unwrap(), 7);
    }
}
