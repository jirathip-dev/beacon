# Job discovery (ADR D12, issue #39)

How it is used, the shapes, what discovery reads, the statecharts, the module boundaries and the tests. The
decisions and the options turned down are in `adr.md`. The exact schema additions are in
`schema-additions.json`.

## How the callers read

### The owner, on the host

```sh
$ beacon serve --config ~/beacon-config/monitors.json          # no new flag: discovery is always on
serve macmini on 127.0.0.1:7878: 3 checks (…), metrics every 60s (/), jobs every 60s

$ beacon jobs                                                   # one reading of this host, plus its jobs block
unreviewed  up    com.jirathip.ttyd        running                  keepalive
unreviewed  up    com.jirathip.boat-rt     last exit 0              every 1800s
kept        up    dev.drover.tick          last exit 0              every 300s      "dev.drover.tick" in Mini
kept        down  dev.drover.clean         last exit 1              every 86400s    "dev.drover.clean" in Mini
kept        down  dev.cooee.cooee          not loaded               -               "dev.cooee.cooee" in Mini
ignored     up    com.jirathip.caffeinate  running                  keepalive
removed     -     dev.old.job              not loaded               -

$ beacon jobs keep com.jirathip.ttyd --name ttyd --group Mini   # also: ignore <label>, removed <label>
~ macmini: keep com.jirathip.ttyd as "ttyd" in Mini
git -C ~/beacon-config commit -m "beacon jobs: keep com.jirathip.ttyd" monitors.json && git -C ~/beacon-config push
$ beacon jobs keep com.jirathip.ttyd --name ttyd --group Mini   # again: nothing to do
= macmini: keep com.jirathip.ttyd as "ttyd" in Mini
```

`--config` defaults to `$BEACON_CONFIG` and `--host` to the short hostname, as for `run`. Exit 0, or 2 on a
config or usage error. The list is a review list, so a down job does not change the exit code.

```sh
$ curl -s http://127.0.0.1:7878/status
{"version":1,"host":"macmini","checks":{
 "disk":{"status":"up","message":"71% used","since":"2026-11-20T09:41:10Z"},
 "job-com-jirathip-ttyd":{"status":"up","message":"running","since":"2026-11-20T09:41:11Z","name":"com.jirathip.ttyd"},
 "job-dev-drover-clean":{"status":"down","message":"last exit 1","since":"2026-11-20T09:41:11Z","name":"dev.drover.clean","group":"Mini"}}}
```

### `serve.rs`: one more thread, beside the sampler

```rust
let reviews = Mutex::new(Arc::new(host.jobs.clone()));              // the reload thread swaps it; this thread clones it
let mut announced = tx.map(|_| Announced::load(&state_dir, name));      // only with --notify
s.spawn(|| jobs::watch(&Manager::here(), || lock(&reviews).clone(), |rows| {
    if let Some(announced) = &mut announced {
        let fresh = announced.admit(rows.iter().filter(|r| r.group.is_none()).map(|r| r.name.clone()));
        for row in rows.iter().filter(|r| fresh.contains(&r.name)) { tx.send(Event::job(name, row)) }
    }                                                               // admit: the file first, then the webhook
    let mut l = lock(live);                                         // after the disk write: no lock across disk
    let changed = l.body.set_jobs(&rows, unix_now());               // record each row; drop job rows not reported
    for row in &rows { l.own.check(row.id.as_str(), row.status, &row.message).map(|e| tx.send(e)); }
    if changed { status.set(l.body.render()) }
}));
```

```rust
// jobs.rs: the loop serve spawned. It owns the Watcher, so there is no lock on it.
pub fn watch(manager: &Manager, reviews: impl Fn() -> Arc<BTreeMap<Label, Review>>, mut publish: impl FnMut(Vec<Row>)) {
    let mut watcher = Watcher::default();
    loop {
        if let Some(listing) = manager.list() {                     // one process a cycle
            for job in watcher.due(&listing, Instant::now()) {      // new, changed, or a scheduled job's interval is up
                watcher.note(job, manager.read(&job.label), Instant::now());   // one process each
            }
            publish(watcher.rows(&listing, &reviews()));
        }                                                           // a failed listing changes nothing; logged once per outage
        thread::sleep(every(JOBS_EVERY_S));
    }
}
```

On a reload, serve sets `reviews` and keeps the job rows: `StatusBody::keep` drops the results of removed
checks only.

### `notify.rs`: the same announcement for another host's jobs

```rust
// in notify::watch, after a good poll of host `h`; `announced` is Announced::load(&state_dir, h)
let fresh = announced.admit(last.values().filter(|c| c.group.is_none()).filter_map(|c| c.name.clone()));
for (id, c) in last.iter().filter(|(_, c)| c.name.as_ref().is_some_and(|n| fresh.contains(n))) {
    tx.send(Event::job(h, id, c))
}
```

### The app (`BeaconKit`)

```swift
// Refresher.run: every host in `hosts` is fetched, not only the ones a heartbeat names
var rows = monitorRows(config, probes, replies, previous)          // as today, each heartbeat row now with its host
rows += jobRows(replies, previous: previous)                       // every /status row that has a `name`
```

`jobRows`: a reply's rows with a `name` become `MonitorResult(name, group ?? "Unreviewed", …, host)`. First
come every host's kept rows, then every host's unreviewed rows, each by host and then by name. The list view
groups by first appearance, so Unreviewed is the last card with no view change. For an unreachable host, the
previous snapshot's rows of that host that no monitor made are kept as down, "host unreachable: <reason>".
`Model.host(of:)` goes away: a row carries its host, so job rows open the host detail and are listed on it
like heartbeat rows.

## Shapes

### Config: `hosts.<name>.jobs` (read by serve and `beacon jobs`, validated by `check`, ignored by the app)

```json
"hosts": {
  "macmini": {
    "url": "https://macmini.tail1234.ts.net/status",
    "checks": [],
    "jobs": {
      "com.jirathip.caffeinate": "ignore",
      "dev.drover.tick": { "keep": { "name": "dev.drover.tick", "group": "Mini" } },
      "dev.old.job": "removed"
    }
  }
}
```

That block is also the fixed form the edit commands write: one label a line, sorted by label, indented two
spaces under `"jobs"`, which sits at the indentation of the host's own fields. Rules `beacon check` adds
(exit 2):

- A label is 1 to 200 characters with no whitespace and no control character.
- A kept job's `name` and `group` have the bounds of a monitor's (60 and 40 characters).
- No `checks[].id` starts with `job-`.

### `/status`: two optional fields on a row

| field | on | value |
|---|---|---|
| `name` | every job row | the kept name, else the label |
| `group` | kept job rows | the kept group; absent means unreviewed |

Rows of `checks` have neither. Messages: `running`, `not running`, `not running, last exit 1`, `last exit 0`,
`last exit 1`, `last exit: signal 9`, `no run in 2 intervals of 300s`, `not loaded`.

### The webhook (D11's body, one new kind)

```json
{"kind":"job","host":"macmini","check":"job-com-jirathip-ttyd","status":"up","message":"new job found: com.jirathip.ttyd","at":"2026-11-20T09:41:11Z"}
```

### The announced file: `~/.local/state/beacon/jobs/<host>.json`

```json
{"version":1,"announced":["com.jirathip.boat-rt","com.jirathip.ttyd"]}
```

Written to a temporary file and renamed. An unreadable file or another version counts as missing (logged
once), so the next report is a baseline.

### Rust (`cli/src`)

```rust
// config.rs
pub struct Host { /* url, timeout_s, checks, disks */ #[serde(default)] pub jobs: BTreeMap<Label, Review> }
pub struct Label(String);                              // checked on parse
#[derive(Deserialize, PartialEq, Clone)] #[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Review { Keep { name: String, group: String }, Ignore, Removed }

// jobs.rs: the rule and the memory between cycles; no I/O except through Manager
pub enum Kind { KeepAlive, Scheduled { period_s: u64 }, OnDemand }
pub enum LastExit { Clean, Failed(String) }            // Failed: the listing's word for the message ("1", "signal 9")
pub struct Listed { pub label: Label, pub running: bool, pub last_exit: LastExit }   // one line of the listing
pub struct Detail { pub kind: Kind, pub runs: String }                               // one read; `runs` is only compared
pub struct Row { pub id: Id, pub name: String, pub group: Option<String>, pub status: Status, pub message: String }

pub fn judge(kind: &Kind, job: &Listed, missed: bool) -> (Status, String);          // the table in ADR §4
pub fn id(label: &Label, shared: bool) -> Id;                                        // job-<slug>, ADR §8

#[derive(Default)] pub struct Watcher { /* per label: its Listed, its Detail, when it was read, idle reads (0..=2) */ }
impl Watcher {
    pub fn due<'a>(&self, listing: &'a [Listed], now: Instant) -> Vec<&'a Listed>;
    pub fn note(&mut self, job: &Listed, detail: Option<Detail>, now: Instant);      // None: judged as OnDemand
    pub fn rows(&mut self, listing: &[Listed], reviews: &BTreeMap<Label, Review>) -> Vec<Row>;
}

pub enum Manager { Launchd { tool: String, uid: String }, Systemd { tool: String, units: PathBuf } }
impl Manager {
    pub fn here() -> Manager;                          // $BEACON_LAUNCHCTL or $BEACON_SYSTEMCTL names the tool and picks
                                                       // the manager; with neither set, target_os picks
    pub fn list(&self) -> Option<Vec<Listed>>;         // probe::output, 10 s; without the left-out labels
    pub fn read(&self, label: &Label) -> Option<Detail>;
}

// jobs/launchd.rs, jobs/systemd.rs: text in, Listed / Detail out. Both compile and are tested on both systems.
pub fn listing(text: &str) -> Vec<Listed>;
pub fn detail(text: &str) -> Option<Detail>;

// jobs/edit.rs: text in, text out
pub fn set(config: &str, host: &Id, label: &Label, review: &Review) -> Result<String, String>;

// state.rs
pub struct Announced { /* path, labels, baseline */ }
impl Announced {
    pub fn load(dir: &Path, host: &str) -> Announced;
    pub fn admit(&mut self, unreviewed: impl Iterator<Item = String>) -> Vec<String>;   // the new ones; none at a baseline
}

// status.rs
struct CheckResult { status, message, since,
                     #[serde(skip_serializing_if = "Option::is_none")] name: Option<String>,
                     #[serde(skip_serializing_if = "Option::is_none")] group: Option<String> }
impl StatusBody { pub fn set_jobs(&mut self, rows: &[Row], now: u64) -> bool; }       // true: re-render
```

### Swift (`app/BeaconKit`)

```swift
public struct CheckResult: Decodable { /* status, message, since */ public let name: String?; public let group: String? }
public struct MonitorResult: Codable { /* name, group, state, detail, since */ public let host: String? }
extension MonitorResult { public static let unreviewed = "Unreviewed" }
```

`MonitorsConfig` does not change: `jobs` is an unknown key, which `Decodable` ignores (D1).

## What discovery reads

### macOS (seen on the Mini, 2026-11-20: `list` 6 ms, `print` 5 ms)

| what | from | how |
|---|---|---|
| the jobs, running, last exit | `launchctl list`, once a cycle | tab-separated `PID Status Label`; PID `-` is not running; Status 0 is clean, a negative one a signal |
| left out | the label | `com.apple.*`, `application.*`, `dev.beacon.serve` |
| KeepAlive | `launchctl print gui/<uid>/<label>` | the line `properties = …` names `keepalive` |
| interval | same | `run interval = 300 seconds` |
| calendar | same | an `event triggers` entry with `stream = com.apple.launchd.calendarinterval`; the interval from its `descriptor` keys: `Month` 366 d, else `Day` 31 d, else `Weekday` 7 d, else `Hour` 1 d, else 1 h; of several entries the longest |
| run count | same | `runs = 170` |

```
$ launchctl list                         $ launchctl print gui/501/dev.drover.clean     (the lines read, of about 90)
PID     Status  Label                        runs = 4
9762    0       dev.drover.tick              properties = runatload | abandon process group | inferred program
-       1       dev.drover.clean             event triggers = { … stream = com.apple.launchd.calendarinterval
50226   -9      dev.shed.serve                                    descriptor = { "Minute" => 30  "Hour" => 3 } … }
```

The uid is `id -u`, read once. `print` exits 113 for a job that is gone; that read is `None`.

### Linux (from systemd's documentation; slice 5 captures the fixtures on a boat first)

| what | from | how |
|---|---|---|
| the jobs | the folder `~/.config/systemd/user`, read each cycle | its `*.service` and `*.timer` files; not `beacon-serve.service` |
| running, last exit | `systemctl --user list-units --type=service,timer --all --plain --no-legend`, once a cycle | `ACTIVE` is `active`: running; `failed`: failed. A timer's job takes both from the service it starts |
| KeepAlive | `systemctl --user show <unit> -p …` | a service whose `Restart` is not `no` |
| scheduled, interval | same, on the timer | `TimersMonotonic`'s repeating value; for `TimersCalendar`, the time from its last trigger to its next elapse |
| run count | same, on the service | `InactiveExitTimestampMonotonic` |

A unit in the folder that systemd has not loaded is not a job, as a plist that was never bootstrapped is not.

## Statecharts

### One label, as serve sees it each cycle

```
                     the config's entry for the label
              none              keep                 ignore     removed
 listed       Unreviewed        Kept                 Hidden     Unreviewed (it is back)
              row, no group     row, name + group    no row     row, no group
 not listed   no row            Kept, vanished       no row     no row
                                down "not loaded"
```

`beacon jobs keep|ignore|removed` moves a label between columns; launchd or systemd moves it between rows.
A row's status is `judge(kind, listed, missed)`. On the notifying host, a label's first time in an Unreviewed
cell sends `new job found` once, ever (the announced file).

### The discovery thread

```
 start ─▶ Listing ── tool fails or times out (10 s) ──▶ keep the last rows; log once per outage ─┐
             │ ok                                                                               │
             ▼                                                                                  │
          Reading: for each job that is new, whose line changed, or whose interval is up        │
             │        (a read that fails or does not parse: the job is judged as on demand)     │
             ▼                                                                                  │
          Publishing: rows ─▶ announce new unreviewed labels (file, then webhook)               │
             │             ─▶ record; drop rows not reported; re-render only on a change        │
             ▼                                                                                  │
          Sleeping 60 s ◀───────────────────────────────────────────────────────────────────────┘
```

### A scheduled job's missed counter (in memory)

```
 first read ─▶ Counting(0)
 Counting(n) ── its interval passes ─▶ read ── run count changed ─▶ Counting(0)
                                           └── unchanged ─────────▶ Counting(n+1); at 2: Missed (down)
 Counting(n) ── its line changed, so it is read early ── changed ─▶ Counting(0);  unchanged ─▶ Counting(n)
 Missed      ── read every cycle ── run count changed ─▶ Counting(0)
 the job leaves the listing, or serve restarts ─▶ forgotten
```

The interval is never shorter than the cycle, and it is measured on the monotonic clock, which stands still
while the host sleeps.

### The app: a job row

```
 host reply 200/304:  row has `name` and `group` ─▶ shown in that group        row has `name` only ─▶ Unreviewed
                      row has no `name` ─▶ not a row by itself (a heartbeat monitor may name it)
 host unreachable:    the last snapshot's job rows of that host ─▶ down "host unreachable: <reason>"
```

## Module boundaries

```
cli/src/jobs.rs           Kind, Listed, Detail, Row, judge, id, Watcher, Manager, watch
cli/src/jobs/launchd.rs   parse `launchctl list` and `print`
cli/src/jobs/systemd.rs   parse `systemctl list-units` and `show`; list the unit folder
cli/src/jobs/edit.rs      replace one host's `jobs` block in the config text
cli/src/jobs/cli.rs       `beacon jobs`: parse its options, print the list, apply an edit, print the commit line
docs/jobs.md              the command's reference (the README links to it)
```

An arrow means "may import":

```
main → jobs::cli                                      jobs::cli → {config, jobs, jobs::edit, out}
serve → {jobs, state (Announced), notify (Event)}     notify → state (Announced)
jobs → {config (Label, Review, Id), status (Status), probe (output only), out, serve (every only),
        jobs::launchd, jobs::systemd}
jobs::launchd, jobs::systemd → jobs (its types)       jobs::edit → config (Id, Label, Review)
```

- `jobs` does not import `notify` or `state`, and from `serve` only `every` (the tests' speed-up), as
  `notify` does: serve hands it two closures, as it hands `notify::watch` one. So `beacon jobs` and serve run
  the same listing, reads and rule.
- `probe` stays the only module that starts a process, and `state` the only one that writes under the state
  folder. The edit commands write the config file from `jobs::cli`; serve never writes it.
- The parsers and `jobs::edit` are pure: text in, values or text out.
- App: `Beacon` and `BeaconWidget` are as they are; only `BeaconKit` changes.

## Tests (the issue's AC)

Fixtures live under `cli/tests/fixtures/jobs/`: `launchd/` and `systemd/` hold captured listings and reads,
and one fake tool answers `list`, `print`, `list-units` and `show` from the folder in `$FAKE_JOBS`, so a test
changes a job by rewriting a file. Tests are in `cli/tests/jobs.rs` and run serve with `BEACON_SPEEDUP=60`.

| AC | test |
|---|---|
| KeepAlive up and down | the listing with and without a PID for a `keepalive` job |
| scheduled, last exit ≠ 0, down | Status 1 for a `run interval` job |
| missed 2 intervals, down | `runs` unchanged across two reads of a 60 s job; a third read with a new count is up |
| vanished, down | a kept label not in the listing: down, `not loaded` |
| `removed` vanished, not shown | the same with `"removed"`: no row |
| ignored, not shown | a listed label with `"ignore"`: no row |
| unreviewed, shown | a listed label with no entry: a row with `name` and no `group` |
| both systems | every test runs on every OS: the variable that names the fake tool picks the manager |
| one webhook, none after a restart | start (baseline, nothing sent), add a job: one `job` POST; restart: none; add another: one |
| edits round-trip | `keep`, `ignore`, `removed` on the example and on a one-line host: `beacon check` exits 0 and the text outside the block's byte range is equal; a second run leaves the file equal |
| budget with discovery on | `budget.rs` gives its serve the fake tool with 50 jobs, 10 of them scheduled |
| the app shows Unreviewed | `RefresherTests`: rows from `name` and `group`, the unreachable carry-over; `.drover/verify/status` gains one kept and two unreviewed job rows (one down) for the 390 shot |

Every other test that starts serve points the tool at an empty fake, so no real job appears in its `/status`.

## Migration and rollout

- `docs/hosts/mini.json`: its 9 launchd checks and their 9 heartbeat monitors go (the issue says 10; the
  file has 9), replaced by 9 `keep` entries with the same name and group. `doctor`, `clis` and `disk` stay.
- `docs/hosts/boat.json`: unchanged (open question 3).
- `docs/monitors.example.json` gains a small `jobs` block, so both test suites decode one.
- Order on the real hosts: the app, then the CLI on each host, then the config.
