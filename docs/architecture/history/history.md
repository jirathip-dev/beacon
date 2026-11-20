# History, TLS expiry and the prober (issue #42)

How it is used, the shapes, the module boundaries, the statecharts and the tests. The decisions and the
options turned down are in `adr.md` (H1–H8). The wire contracts are `history.schema.json` and
`times.schema.json`.

## How the callers read

### The owner: two optional config fields, no new flag

```json
{ "version": 1,
  "prober": "macmini",
  "monitors": [
    { "type": "http", "name": "Blog", "group": "Web", "url": "https://example.com/" },
    { "type": "http", "name": "API", "group": "Web", "url": "https://api.example.com/health", "cert_warn_days": 30 } ],
  "hosts": { "macmini": { "url": "https://macmini.tail1234.ts.net/status", "checks": [] } } }
```

```sh
$ beacon serve --config ~/monitors.json
serve macmini on 127.0.0.1:7878: 2 checks (…), metrics every 60s (/), probing 3 monitors every 60s
$ curl -s http://127.0.0.1:7878/history
{"version":1,"host":"macmini","gaps":[[1794700000,1794703600]],
 "checks":{"backup":{"since":1794000000,"down":[{"down_at":1794787200,"up_at":1794790800,"message":"exit 1"}]},
           "disk":{"since":1794000000,"down":[]}},
 "monitors":[{"id":"9f2c41d07ab3e815","group":"Web","name":"Blog","since":1794000000,
              "down":[{"down_at":1794900000,"up_at":null,"message":"cert expires in 13d"}]}]}
$ curl -s http://127.0.0.1:7878/history/9f2c41d07ab3e815
{"version":1,"id":"9f2c41d07ab3e815","every_s":60,
 "recent":{"t":[1794899880,1794899940,1794900000],"ms":[212,null,198]},
 "hourly":{"t":[1794895200,1794898800],"avg_ms":[205,201],"max_ms":[412,230]}}
```

Both answer 304 to a matching `If-None-Match`. A host that is not the prober serves `/history` with
`"monitors": []` and has no `/history/<id>`.

### `serve.rs`: one writer of `/status`, which also feeds the history

```rust
fn record(l: &mut Live, cells: &Cells, id: &Id, st: Status, message: String, now: u64) {   // checks; later rules, jobs
    if l.history.record(Key::Check(id), st, &message, now) { cells.history.set(l.history.render()); }
    if l.body.record(id, st, message, now) { cells.status.set(l.body.render()); }
}
```

```rust
// the reload thread's existing 60 s tidy tick: the only place events.json is written
let bytes = { let mut l = lock(live);                                 // no I/O under the lock
              if l.history.tick(unix_now()) { cells.history.set(l.history.render()); }
              cells.history.get() };
state::keep_events(&dir, &bytes);                                      // changed: tmp + rename · same: set mtime to now
state::tidy(&dir);                                                     // also drops times/*.bin not written for 30 days
```

### `prober.rs`: four threads, each owning its monitors' rings

```rust
pub fn run<'s>(s: &'s Scope<'s, '_>, generation: u64, monitors: Vec<Probed>, live: &'s Mutex<Live>, cells: &'s Cells, dir: &'s Path);

// one of the PROBERS = 4 threads; `mine` = the monitors with index % 4 == this thread
let agent = net::agent();                                              // 16 KB buffers, no kept connections
loop {
    for (m, ring, cert) in &mut mine {
        if cert.due() { cert.set(monitor::cert_expiry(&m.target, m.timeout)); }      // http only, hourly
        let seen = monitor::probe(&agent, &m.target, m.timeout, cert.expires());     // (status, message, ms)
        { let mut l = lock(live);
          if l.generation != generation { return; }                                 // a reload replaced this thread
          if l.history.record(Key::Monitor(&m.id), seen.status, &seen.message, unix_now()) { cells.history.set(l.history.render()); } }
        for bytes in ring.put(unix_now() / 60, seen.ms) { m.file.write_all_at(&ring.image()[bytes.clone()], bytes.start) }
        cells.times.set(&m.id, ring.render(&m.id));
    }
    sleep_to_next(EVERY_S);                                            // fixed marks; an overrun starts the next round at once
}
```

### The request loop: two more arms on the same cells

```rust
let r = match request.url() {
    "/status" => status.get(), "/metrics" => metrics.get(), "/config" => config.get(), "/history" => history.get(),
    url => match url.strip_prefix("/history/").and_then(|id| times.get(id)) { Some(r) => r, None => return not_found(request) },
};
```

### The app (`BeaconKit`), only while a detail screen is open

```swift
let history = HostHistory(transport)
switch await history.index(host.historyURL, etag: shown?.etag, timeout: host.timeout_s ?? 5) {    // the replies of HostMetrics
case .fresh(let body, let etag): shown = (body, etag)
case .unchanged: break
case .notServed: note = "no history"                         // 404: a serve from before #42
case .unreachable(let why): note = why
}
let track = body.checks[check.id]                            // a heartbeat row: its own host's /history
         ?? body.monitors.first { $0.group == row.group && $0.name == row.name }?.track   // http/tcp: the prober's
track.uptime(last: 86_400, gaps: body.gaps, now: .now)       // Double? in 0...1 · nil: nothing was watched
track.watched(last: 30 * 86_400, gaps: body.gaps, now: .now) // the seconds really covered, for the label
track.down.suffix(5)                                         // the incident log: down at, up at, duration, message
let times = await history.times(host.historyURL(id: m.id), etag: …)    // http/tcp, while its chart is open
Window(times.recent.t, times.recent.ms, range: .hour)        // 1 h and 24 h: `recent` · 30 d: `hourly`
```

A heartbeat row asks its own host. An http or tcp row asks `hosts[config.prober]`; with no `prober` the
screen shows no history. `Refresher`, background refresh and the widget fetch none of this. The one change
in `Refresher` is the certificate rule in `http()` (H5).

## Shapes

### Config: the fragment impl adds to `../beacon/monitors.schema.json`

```json
"prober":         { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,63}$" }
"cert_warn_days": { "type": "integer", "minimum": 0, "maximum": 365, "default": 14 }
```

`prober` is top-level; `cert_warn_days` is on an `http` monitor. Rules the schema cannot express, enforced
by `beacon check` (exit 2): `prober` is a key of `hosts`; with `prober` set, no two http/tcp monitors have
the same `<group>/<name>`.

### Wire: `history.schema.json`, `times.schema.json`

Times are Unix seconds, as in `/metrics`. Rules the schemas cannot express:

- In `down`, incidents are oldest first and do not overlap; only the last may have `up_at: null`.
- `since` is when this key's knowledge starts: first seen, or the end of the oldest incident dropped.
- In `/history/<id>` every array of `recent` has `recent.t.count` entries, and the same for `hourly`.
  `ms: null` is a probe with no response (timeout, refused). A response with the wrong status has a time.
  A minute that was not probed and an hour with no response are absent, not null.
- `hourly.t` is the start of the hour; the last entry is the running hour.

Uptime of a key over the last `W` seconds, worked out by the app:

```
window  = [max(now − W, since), now]
unknown = seconds of `gaps` inside window
down    = seconds of `down` inside window (an open incident ends now), not counting seconds inside a gap
uptime  = 1 − down ÷ (window − unknown)          nil when window − unknown ≤ 0
```

### Rust (`cli/src`)

```rust
// history.rs: pure, no clock, no I/O
pub const INCIDENTS: usize = 64;   pub const GAPS: usize = 128;   pub const KEEP_S: u64 = 30 * 86_400;
pub const GAP_S: u64 = 180;        pub const MESSAGE: usize = 120;

#[derive(Serialize, Deserialize)]
pub struct History { version: u8, host: String, gaps: VecDeque<(u64, u64)>, checks: BTreeMap<String, Track>,
                     monitors: Vec<MonitorTrack>, #[serde(skip)] alive: u64 }
pub struct Track { since: u64, down: VecDeque<Incident> }            // private fields: at most INCIDENTS
pub struct Incident { down_at: u64, up_at: Option<u64>, message: String }
pub struct MonitorTrack { id: MonitorId, group: String, name: String, #[serde(flatten)] track: Track }
pub struct MonitorId(String);                                        // 16 hex: FNV-1a 64 of "<group>/<name>"
pub enum Key<'a> { Check(&'a Id), Monitor(&'a MonitorId) }

impl History {
    pub fn load(host: &str, events: Option<&[u8]>, alive: u64) -> History;   // unreadable or another version: empty
    pub fn record(&mut self, key: Key, status: Status, message: &str, now: u64) -> bool;   // true: it changed
    pub fn keep(&mut self, checks: &[&Id], monitors: &[Probed]);     // on reload, as StatusBody::keep
    pub fn tick(&mut self, now: u64) -> bool;                        // a gap when now − alive > GAP_S; drops what is past KEEP_S
    pub fn render(&self) -> Rendered;                                // the /history body and the bytes of events.json
}

pub struct Times(Box<[u8; 5776]>);                                   // the ring file's image
impl Times {
    pub fn load(file: Option<&[u8]>) -> Times;                       // another size or magic: empty
    pub fn put(&mut self, minute: u64, ms: Option<u16>) -> [Range<usize>; 3];   // the bytes it changed
    pub fn render(&self, id: &MonitorId) -> Rendered;                // the /history/<id> body
}

// monitor.rs: the only module that probes the network
pub enum Target { Http { url: HttpsUrl, expect_status: Option<u16>, keyword: Option<String>, cert_warn_days: u16 },
                  Tcp { host: String, port: u16 } }
pub struct Probed { pub id: MonitorId, pub group: String, pub name: String, pub target: Target, pub timeout: Duration }
pub struct Seen { pub status: Status, pub message: String, pub ms: Option<u16> }   // ms: a response arrived
pub fn probe(agent: &Agent, target: &Target, timeout: Duration, cert_expires: Option<u64>) -> Seen;
pub enum Expiry { Fine, At(u64) }                                    // At: notAfter, inside the warning window
pub fn cert_expiry(target: &Target, timeout: Duration) -> Option<Expiry>;   // None: could not tell; keep the last answer

// net.rs: the only module that builds a TLS client
pub fn agent() -> Agent;                                             // moved from notify.rs; 16 KB buffers
pub fn roots() -> Arc<RootCertStore>;                                // webpki's, or $SSL_CERT_FILE's

// config.rs (amended)
pub struct MonitorsConfig { /* … */ pub prober: Option<Id> }
pub struct HttpMonitor { /* … */ #[serde(default = "fourteen")] pub cert_warn_days: u16 }
```

The ring file, `times/<id>.bin`, little-endian, 5 776 bytes:

| offset | bytes | what |
|---|---|---|
| 0 | 8 | `BCNT`, then the format version 1 as u32 |
| 8 | 8 | `last`: the minute (Unix seconds ÷ 60) of the newest probe |
| 16 | 2 880 | 1 440 × u16: slot `minute % 1440` holds that probe's ms. `0xFFFF` not probed, `0xFFFE` no response |
| 2 896 | 2 880 | 720 × (avg u16, max u16): slot `hour % 720`. `0xFFFF` no response in that hour |

`put(minute, ms)`: clear the minute and hour slots skipped since `last`, write the minute's slot, work out
its hour's slot again from that hour's minute slots up to this minute, set `last`. It keeps no running sum,
so a restart loses nothing and writing the same minute twice changes nothing.

### Swift (`app/BeaconKit/History.swift`)

```swift
public struct HistoryBody: Decodable, Sendable { let version: Int; let host: String; let gaps: [[Int]]
                                                 let checks: [String: Track]; let monitors: [MonitorTrack]? }
public struct Track: Decodable, Sendable { let since: Int; let down: [Incident] }
public struct Incident: Decodable, Sendable { let down_at: Int; let up_at: Int?; let message: String }
public struct MonitorTrack: Decodable, Sendable { let id, group, name: String; let track: Track }   // flat on the wire
public struct TimesBody: Decodable, Sendable { let version: Int; let id: String; let every_s: Int
    let recent: Recent; let hourly: Hourly
    struct Recent: Decodable, Sendable { let t: [Int]; let ms: [Double?] }
    struct Hourly: Decodable, Sendable { let t: [Int]; let avg_ms, max_ms: [Double?] } }
// MonitorsConfig gains `prober: String?`; HTTPMonitor gains `cert_warn_days: Int?`; Host gains `historyURL`.
// Transport.get's result gains the leaf certificate's notAfter (nil for a 304 from cache or plain http).
```

## Module boundaries

An arrow means "may import". Everything not listed is as in `../beacon/modules.md`.

```
cli:  serve   → {history, prober, state, …as before}
      prober  → {monitor, history (Times, MonitorId, Key), net, state (paths), status}   threads; writes only its own ring files
      monitor → {net, config, status}        probes: http (ureq), tcp (std), the certificate handshake (rustls)
      net     → ureq, rustls                 TLS roots and the agent, nothing else
      notify  → net                          its private agent() moves there
      history → {status (Status, Rendered), config (Id)}        pure
      state   → files only: events.json (read, write, touch), the 30-day sweep of times/
      run, check: unchanged. `beacon run` probes no monitor.

app:  Beacon views → BeaconKit/History.swift (bodies, uptime, fetch) → Transport
      Refresher → Transport (the certificate rule in http()).  BeaconWidget: unchanged, never reads history.
```

- Only `serve.rs`'s `record` and the prober call `History::record`. #41's rules and #39's jobs call `record`.
- Only the reload thread writes `events.json`. Only a prober thread writes a ring, and only its own.
- `history.rs` never reads a clock or a file: `now` and the bytes are handed to it, so its tests are tables.

## Statecharts

### One key (a check, a rule, a job or a monitor) in `History`

```
 not known ── first result: up ────▶ Up     (since = now)
           ── first result: down ──▶ Down   (since = now; incident {down_at: now, up_at: null, message})

 Up ── down ──▶ Down: push incident; over 64? drop the oldest and set since = its up_at
 Down ── up ──▶ Up:   the open incident gets up_at = now
 Up ── up, Down ── down (any message) ──▶ no change, nothing rendered, nothing written
 any ── the key leaves the config (reload) ──▶ not known        any ── serve restarts ──▶ the same state, from events.json
 tick: an incident whose up_at is over 30 days old is dropped
```

### The store, on the reload thread's 60 s tick

```
 serve starts ─▶ read events.json ── missing, unreadable, another version ─▶ empty History (logged once)
                      │ ok: alive = the file's modification time
                      ▼
 tick (60 s) ─▶ now − alive > 180 s? ── yes ─▶ gaps.push([alive, now])      (a restart, or the host slept)
                alive = now; drop gaps and incidents past 30 days
                rendered bytes ≠ the bytes last written? ── yes ─▶ write tmp, rename
                                                         ── no ──▶ set the file's modification time to now
```

A crash loses at most the last 60 s of changes, and the next start records that time as part of a gap only
if it is over 180 s. A write that fails is logged once per outage (as the `git pull` line) and tried again
on the next tick.

### One prober thread, and a monitor's certificate

```
 for each of its monitors:  cert due (first round, then hourly, http only)?
                                 handshake with the clock cert_warn_days ahead
                                   passes ─▶ Fine      fails "expired", notAfter ─▶ Expiring(notAfter)
                                   anything else ─▶ keep the last answer (the probe below reports the fault)
                            probe (new connection, timeout_s):
                                 no response ─────────────────────────▶ down, its reason, ms = none
                                 response, wrong status or keyword ───▶ down, D5's message, ms
                                 response ok, Expiring(notAfter) ─────▶ down "cert expires in <n>d", ms    n = (notAfter − now) ÷ 86 400
                                 response ok ─────────────────────────▶ up, ms
                            record ─▶ ring.put ─▶ write the changed bytes ─▶ render the monitor's cell
 then sleep to the next 60 s mark (none left: start at once)          reload: the thread ends at its next record
```

### The app: a detail screen

```
 opens ─▶ GET <host url with `history` as its last segment>, If-None-Match
     200 ─▶ Shown(index)      304 ─▶ Shown(the index it has)      404 ─▶ NoHistory
     timeout, other status, undecodable ─▶ Unreachable(why), over the last Shown if any
 an http/tcp monitor's chart opens ─▶ GET …/history/<id> the same way, then every 60 s while it is open
 screen closes ─▶ nothing is fetched
```

## Tests (the issue's AC)

| what | test |
|---|---|
| shapes | `docs/history.example.json` and `docs/times.example.json` are the exact bytes `render` gives for fixed inputs. The CLI test asserts the bytes and decodes them into strict mirrors of the schemas; the BeaconKit test decodes the same files and checks `uptime` against numbers worked out by hand (an open incident, a gap inside an incident, a key younger than the window). |
| bounds | 100 incidents on one key: 64 kept and `since` moved. 2 000 minutes into a ring: 5 776 bytes, `recent.t` has 1 440 entries. A skipped day: its slots are gone. Incidents and gaps past 30 days: dropped by `tick`. |
| restart | `cli/tests/serve.rs`: a check goes down, serve is killed and started again: the incident is still open with its first `down_at`, and `gaps` has the stop. A corrupt `events.json`: serve starts and serves an empty history. |
| prober | Local TLS hosts as in `notify.rs`: an http and a tcp monitor get times; a hanging host gives `ms: null` and an incident; a host that is not the `prober` serves no monitors. |
| TLS expiry | A certificate made at test time with the `openssl` CLI for 10 days (no stored fixture can be "about to expire"): down with `cert expires in 9d` or `10d`; with `cert_warn_days: 5` it is up. The app: the same rule on a mocked `notAfter`. |
| budget | `budget.rs`'s `idle` config with `"prober": "box"` and 20 http monitors on a local TLS server passes every line. `/history` and one `/history/<id>` join the unchanged-poll and p99 tests. The soak asserts `history/` stays under 1 MB. `BEACON_OVER_BUDGET=history` must fail it. |
| app screens | After design: verify screenshots at 390 from `.drover/verify/` fixtures of both bodies. |
