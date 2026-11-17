# Host metrics (ADR D10, issue #13)

Everything about `/metrics` in one file: how it is used, the shapes, what the sampler reads, the statechart
and the tests. The wire contract is `metrics.schema.json`. The decisions and their options are in `adr.md` D10.

## How the callers read

### The owner, on the host

```sh
$ beacon serve --config ~/monitors.json           # no new flag: sampling is always on
serve macmini on 127.0.0.1:7878: 2 checks (backup every 300s, disk every 60s), metrics every 60s (/)
$ sleep 130; curl -si http://127.0.0.1:7878/metrics
HTTP/1.1 200 OK
ETag: "5c0e9a1f33b27d48"
Cache-Control: no-cache
Content-Type: application/json

{"version":1,"host":"macmini","every_s":60,"t":[1794787200,1794787260,1794787320],"load1":[1.52,1.61,1.4],"load5":[1.8,1.79,1.75],"load15":[2.01,2.0,1.99],"cpu_pct":[null,12.5,9.8],"mem":{"total_bytes":34359738368,"used_bytes":[16622419968,16631840768,16618512384]},"swap":{"total_bytes":2147483648,"used_bytes":[718404485,718404485,718404485]},"disks":{"/":{"used_bytes":[358843744256,358843801600,358843867136],"free_bytes":[135488622592,135488565248,135488499712]}}}
$ curl -si -H 'If-None-Match: "5c0e9a1f33b27d48"' http://127.0.0.1:7878/metrics | head -1
HTTP/1.1 304 Not Modified                         # until the next sample, at most 60 s later
```

The config gains one optional field, read only by serve:

```json
"hosts": { "macmini": { "url": "https://macmini.tail1234.ts.net/status", "disks": ["/", "/Volumes/backup"], "checks": [] } }
```

Absent `disks` means `["/"]`. `"disks": []` samples no disk. Nothing else is configurable.

### `serve.rs`: the sampler thread, beside the check threads

```rust
let (mut sampler, mut ring) = (Sampler::new(), Ring::new());   // owned by this thread: no lock on the ring
for tick in 0.. {
    let disks = config.current().disks.clone();                // read each tick: the reload, once built, restarts nothing here
    ring.push(sampler.sample(unix_now(), &disks));             // OS reads; a failed read is a None, never a panic
    metrics.set(ring.render(&host_name, &disks));              // bytes + ETag, swapped under the cell's mutex
    sleep_until(started + (tick + 1) * EVERY_S);               // fixed schedule: a slow read does not shift it
}
```

### `serve.rs`: the request loop, one arm for both endpoints

```rust
let cell = match request.url() { "/status" => &status, "/metrics" => &metrics, _ => return not_found(request) };
let r = cell.get();                                            // an Arc clone: never waits on a check or a read
if request.header("If-None-Match") == Some(&r.etag) { respond(304) }
else { respond(200, &r.body, [etag(&r.etag), NO_CACHE, JSON]) }
```

### The app (`BeaconKit`, built by #14): only while the host detail screen is open

```swift
let url = URL(string: "metrics", relativeTo: host.url)!.absoluteURL   // …/status → …/metrics: no new config field
switch await HostMetrics(transport).fetch(url, etag: shown?.etag, timeout: host.timeout_s ?? 5) {
case .fresh(let body, let etag): shown = (body, etag)          // 200: MetricsBody decoded, unknown fields ignored
case .unchanged:                 break                         // 304: keep drawing `shown`
case .notServed:                 note = "This host's beacon serve has no metrics"   // 404: a serve from before #13
case .unreachable(let why):      note = why                    // keep drawing `shown` if there is one
}
```

`Refresher`, background refresh and the widget never fetch `/metrics`.

## Shapes

### Wire: `metrics.schema.json`

Columns: `t`, `load1`, `load5`, `load15`, `cpu_pct`, `mem.used_bytes`, `swap.used_bytes` and each mount's
`used_bytes` and `free_bytes` are arrays with one entry per sample, oldest first, at most 1440. Rules the
schema cannot express:

- Every array has exactly `t.count` entries. The CLI builds them from one ring, so it holds by construction,
  and the golden test checks it. The app treats a body that breaks it as undecodable.
- `disks` has exactly the mounts of the config serve runs now. A mount added by a reload has `null` for the
  samples before it; a removed mount leaves the response.

Units: `t` is Unix seconds (an integer, unlike `/status`'s RFC 3339 `since`: 1440 of them are parsed per
fetch). Sizes are bytes. `cpu_pct` is 0–100 with one decimal. Loads have two decimals.

### Rust (`cli/src`)

```rust
// config.rs (amended)
pub struct Host { /* url, timeout_s, checks as before */ #[serde(default = "root")] pub disks: Vec<MountPath> }
pub struct MountPath(String);          // starts with '/', 1..=255 chars, no control character; checked on parse
// validate(): at most 8 disks per host, no duplicates

// metrics.rs: pure
pub const CAPACITY: usize = 1440;      // 24 h at one sample a minute
pub const EVERY_S: u64 = 60;

pub struct Sample {                    // one reading of the host; None = that read failed
    pub t: u64,                        // Unix seconds
    pub load: Option<[f32; 3]>,        // 1, 5, 15 min
    pub cpu_pct: Option<f32>,          // mean since the previous sample; None on the first
    pub mem: Option<Gauge>,
    pub swap: Option<Gauge>,
    pub disks: BTreeMap<MountPath, DiskUsage>,   // a mount whose read failed is absent
}
pub struct Gauge { pub total: u64, pub used: u64 }        // bytes
pub struct DiskUsage { pub used: u64, pub free: u64 }     // bytes; used + free = the volume's size

pub struct Ring(VecDeque<Sample>);     // private field: it cannot hold more than CAPACITY
impl Ring {
    pub fn new() -> Ring;
    pub fn push(&mut self, sample: Sample);                               // full: drops the oldest first
    pub fn len(&self) -> usize;
    pub fn render(&self, host: &str, disks: &[MountPath]) -> Rendered;   // rows → columns → bytes + ETag
}

// sampler.rs: OS reads, and the pure parsers of what they return
pub struct Sampler { prev: Option<CpuTicks> }
impl Sampler {
    pub fn new() -> Sampler;
    pub fn sample(&mut self, t: u64, disks: &[MountPath]) -> Sample;     // never panics
}
pub struct CpuTicks { pub busy: u64, pub total: u64 }                    // cumulative since boot
pub fn cpu_pct(prev: CpuTicks, now: CpuTicks) -> Option<f32>;            // None: no tick passed, or a counter went back

pub fn linux_load(loadavg: &str) -> Option<[f32; 3]>;                    // parsers: pure, compiled on every OS,
pub fn linux_cpu(stat: &str) -> Option<CpuTicks>;                        // so each OS's CI run tests both paths
pub fn linux_mem(meminfo: &str) -> Option<(Gauge, Gauge)>;               // (mem, swap)
pub fn macos_load(sysctl: &str) -> Option<[f32; 3]>;
pub fn macos_mem(vm_stat: &str, memsize: &str) -> Option<Gauge>;
pub fn macos_swap(sysctl: &str) -> Option<Gauge>;
pub fn df(output: &str) -> Option<DiskUsage>;                            // the same on both

// probe.rs (amended): still the only module that starts a process
pub fn output(program: &str, args: &[&str], timeout: Duration) -> Option<String>;   // None unless exit 0 in time
// probe::run(command, timeout) stays, built on the same spawn-wait-kill helper
```

`Rendered` (bytes + ETag) is the M0 type from `status.rs` (`types.md`). `/status` and `/metrics` each have
one cell of it in serve, so "cached like `/status`" is one mechanism, not two.

### Swift (`app/BeaconKit`, built by #14)

```swift
struct MetricsBody: Decodable {
    let version: Int; let host: String; let every_s: Int      // version != 1 throws, as in D1
    let t: [Int]; let load1, load5, load15, cpu_pct: [Double?]
    let mem, swap: Gauge; let disks: [String: Disk]
    struct Gauge: Decodable { let total_bytes: Int?; let used_bytes: [Int?] }
    struct Disk: Decodable { let used_bytes, free_bytes: [Int?] }
}
```

## What the sampler reads

Only the reads differ by OS (`#[cfg(target_os)]`). Every parser is a pure function of text.

| Value | Linux (files) | macOS (the OS's own tools, by absolute path) |
|---|---|---|
| load 1/5/15 | `/proc/loadavg`: the first three fields | `/usr/sbin/sysctl -n vm.loadavg hw.memsize vm.swapusage`, line 1: `{ 11.26 12.89 15.17 }` |
| cpu ticks | `/proc/stat`, line `cpu `: total = the first 8 fields, busy = total − idle − iowait | the Mach call `host_statistics(HOST_CPU_LOAD_INFO)`: user, system, idle, nice; busy = total − idle |
| memory | `/proc/meminfo`: total = `MemTotal`, used = `MemTotal` − `MemAvailable` | total = `hw.memsize` (line 2); used = (`Anonymous pages` − `Pages purgeable` + `Pages wired down` + `Pages occupied by compressor`) × the page size in the first line of `/usr/bin/vm_stat` |
| swap | `/proc/meminfo`: total = `SwapTotal`, used = `SwapTotal` − `SwapFree` | line 3: `total = 2048.00M  used = 685.12M  free = 1362.88M  (encrypted)` |
| disk, per mount | `df -kP -- <mount>` | `/bin/df -kP -- <mount>` |

- `cpu_pct` = 100 × Δbusy ÷ Δtotal between two samples, so it is the mean of the whole minute on both OSes.
  macOS ticks are 32-bit and wrap after weeks: a counter that went back gives one `null`, then recovers.
- macOS "used" memory is Activity Monitor's "Memory Used". On the Mini at the time of writing: 16.6 GB of 34.4 GB,
  where `top` said 22 G because it counts file cache.
- `df`: the last line's first run of three integers followed by a `NN%` field is blocks, used, available (this
  survives spaces in the filesystem or mount name). used = (blocks − available) × 1024, free = available × 1024.
  Not `df`'s own Used column: on APFS that is one volume's share (for `/` on the Mini it says 13 GiB, while 334 GiB
  of the disk are taken).
- One `df` per mount, so one unmounted path does not hide the others. The mount is passed as an argument after `--`.
  No shell is involved.
- Tools run through `probe::output` with a 5 s kill timeout, so a hung network mount costs one `null`, not the thread.
- On any other OS every read is `None`: serve runs, `/metrics` holds nulls.

## Statechart

### The sampler thread and its ring

```
 serve starts ─▶ Reading (t = now): every read of the table, each ok → value · failed → None
                    │                (a read is logged to stderr when it starts failing and when it recovers)
                    ▼
                 cpu_pct = cpu_pct(previous ticks, ticks)        first sample, or ticks unreadable: None
                    ▼
                 ring.push ── len < 1440 ─▶ appended                          Ring: Empty ─▶ Filling ─▶ Full
                    │        ─ len = 1440 ─▶ oldest dropped, appended                (Full stays Full: 1440)
                    ▼
                 render ─▶ swap bytes + ETag into the /metrics cell
                    ▼
                 Sleeping until start + n × 60 s ─▶ Reading
                    (the host slept past several ticks: one sample on wake, not one per missed tick)
```

Nothing is written to disk. A restart begins with an empty ring. The ring is bounded by count, not by age: after
a host sleep its oldest point can be older than 24 h, and a chart clips to its own window.

### `GET /metrics` (added to the serve process chart in `statechart.md`)

```
 before the first sample ───────────────▶ 200, every array empty        (a few ms after start; never 404 or 503)
 If-None-Match == etag ─────────────────▶ 304                           (never waits on a read)
 otherwise ─────────────────────────────▶ 200 body, ETag, Cache-Control: no-cache
```

### The app: does this host serve metrics? (#14)

```
 screen opens ─▶ GET <url with `metrics` as its last segment>, If-None-Match
     200 + decodes as version 1 ─▶ Charts(body)          304 ─▶ Charts(the body it has)
     404 ─────────────────────────▶ NotServed ("no metrics": a serve from before #13)
     timeout / other status / undecodable ─▶ Unreachable(why), over the last Charts if any
 every 60 s while the screen is open ─▶ the same GET          screen closes ─▶ nothing is fetched
```

## Tests (the issue's AC)

| AC | Test |
|---|---|
| Schema plus golden | `docs/metrics.example.json` is the exact bytes `Ring::render` gives for three fixed samples (the first with no cpu, one with a mount that failed). `cli/tests/metrics.rs` asserts the bytes, decodes them into a strict mirror of the schema (`deny_unknown_fields`) and checks every array has `t.len()` entries. #14's BeaconKit test decodes the same file. |
| Sampler, both OS paths | `cli/tests/sampler.rs` feeds `cli/tests/fixtures/sampler/*.txt` (real `/proc/loadavg`, `/proc/stat` twice, `/proc/meminfo`, `sysctl`, `vm_stat`, `df` from Linux and from the Mini) to the parsers and `cpu_pct`, with the expected numbers. It runs on ubuntu CI and on a Mac alike. One more test calls `Sampler::sample` on the real OS and asserts load, memory and `/` are `Some`. |
| Ring ≤ 1440 | Push 1500 samples: `len() == 1440`, and the rendered `t` starts at the 61st. |
| `curl` on the Mini | `cli/tests/serve.rs` starts `beacon serve --listen 127.0.0.1:0`, reads the port from its first line, and gets `/metrics` until it has one point (200, an ETag), then 304 with that ETag, and 404 for `/x`. The impl lane's proof run is the `curl` at the top of this file after two minutes. |
