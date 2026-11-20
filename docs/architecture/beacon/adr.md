## Summary

Decision: `beacon serve` samples its own host once a minute (load, cpu, memory, swap, disk per mount), keeps the last 1440 samples in memory and serves them at `/metrics` as one JSON with an ETag, cached like `/status`. It reads the OS's own text plus one macOS system call for cpu, so there is no new dependency. The app finds metrics at the host's existing URL with `status` swapped for `metrics`; a 404 means the host has none.

Slices: (1) serve itself with `/status`, as M0 decided it; (2) the metrics shape, ring, schema and golden test; (3) the sampler with macOS and Linux fixtures; (4) the `/metrics` endpoint and the `disks` config field.

Open questions: (1) `beacon serve` is not built yet: the CLI has only `check` and `run`. I made it slice 1 of this issue; say so if you want it as its own issue first. (2) The line cap stands at 1902 of 2000 and this issue adds about 450 lines, so impl will ask for a raise to about 2400.

# ADR: Beacon M0: config schema, host status, app/CLI split

Status: D1–D9 approved (issue #1; third round: q1511 and q1516 applied). D10, host metrics, is proposed by issue #13
and is the only part to review now: read `metrics.md` first, then D10 below. For D1–D9 read `usage.md` first.

## Question

Beacon is an iPhone app that checks things itself, plus a host CLI that reports host
checks. Nothing is hosted and nothing is shared: no GitHub API, no config repo, no token
(amendment q1516). Where does the config live? How does the app learn a host's check
results? What do the schemas look like, and how is the code split? The owner's bar
throughout is minimal, fast and few moving parts.

## Decisions

### D1 `monitors.json` `version: 1`; CLI strict, app tolerant (q1511)

There is one file with one integer `version`. Its shape is in `monitors.schema.json`. Both readers reject
a version they do not know and an unknown monitor `type`. Unknown fields differ by reader:
the CLI (`beacon serve|run|check`) rejects them (`deny_unknown_fields`); the app
(`BeaconKit`) ignores them, in `monitors.json` and in serve's response alike. Monitor types
are a tagged union on `type`: `http`, `tcp`, `heartbeat`. `hosts` sits beside them and
holds each serve host's `url` and its `checks`. A `heartbeat` monitor names a `host` and a
check `id` on it.

Options considered:
- Both readers strict. Rejected (owner, q1511): the app ships through the App Store, so a
  new field would turn every installed app red until its update lands.
- Both readers tolerant. Rejected: a typo (`timeout` for `timeout_s`) would silently take
  the default. `beacon check` is the editor's validator and keeps the typo guard.
- Host checks carry their own name/group and the `heartbeat` type goes away. Rejected: that
  makes two display lists. With the `heartbeat` type, `monitors` stays the single ordered list
  the app shows, and `hosts` stays what serve runs.

Consequences: adding a field is not a version bump (update the CLI first, then the config).
A new monitor type, or a field whose meaning changes, is a version bump.

### D2 The config lives anywhere: an HTTPS URL or a local copy

- **App**: a `ConfigSource` is either `.url(URL, header?)` or `.local`. A URL can be a
  gist raw link, a static host or a file on a tailnet machine. The optional header
  (`Authorization: Bearer …` or similar) is the only secret and lives in the Keychain. `.local` is
  a copy that the user imports from Files, pastes or edits in the app. It is decoded on save, and an invalid edit
  is refused with the decode error. With a URL source, the last good copy and its `ETag` are
  cached in the App Group container. Each refresh sends `If-None-Match`, so a 304 or a
  failed fetch uses the cache, and the screen shows "config from <time>" when the fetch failed.
- **CLI**: `--config <path|https-url>`, else `$BEACON_CONFIG`. For a URL, the optional header
  comes from `$BEACON_CONFIG_HEADER` (`Name: value`). There is no new secret on the host when
  the URL is public or on the tailnet.

An agent edits the file wherever it lives and runs `beacon check <path|url>`. Then it is done.

Rejected: a `beacon-config` repo with a GitHub token (owner, q1516). Rejected: config pushed
from serve to the app (that makes the host the config's source; the app should work with no host).

### D3 Host results: each host runs `beacon serve`; the app polls it

`beacon serve` loads the config, takes `hosts.<this host>.checks` and runs each check on
its own schedule (`every_s`, default 60) inside the process. It keeps the latest result per
check in memory and answers `GET /status` from that cache, so a request never waits on a
check. The response (`status.schema.json`) is one small JSON for all of the host's checks:
`{version, host, checks: {id: {status, message, since}}}`. Every other path gets a 404.

The app treats a `heartbeat` monitor as one field of its host's response. The app fetches
each host once per refresh, alongside the http/tcp probes, and maps the result:
- the host is unreachable, times out (`hosts.<h>.timeout_s`, default 5), returns non-2xx or sends a body that
  does not decode: all of its heartbeat monitors are **down** ("host unreachable: …");
- the check id is absent: **down** ("no result yet"), which is what a just-restarted serve shows;
- otherwise the check's `status` and `message`, with `since` shown as "since <time>".

Serve is reached over Tailscale. The recommended setup has serve listen on `127.0.0.1:7878` (the
default) and `tailscale serve --bg 7878` proxy it, which gives the app HTTPS with a real
certificate and no ATS exception. The tailnet is the access control, so serve has no auth of its own.

Options considered:
- Hosts push to a shared store (the old D2–D4). Rejected (q1516): it needs a store and a token.
- One HTTP endpoint per check. Rejected: N requests per host instead of 1.
- Checks run on each request. Rejected: the app would wait on slow checks, and frequent polls would run the commands over and over.
- `max_age_min` on heartbeats. Dropped. Staleness was for pushed beats. With pull, a dead
  host is unreachable, and every check is killed at its `timeout_s`, so a result cannot hang.
  A panicking check thread aborts the process (`panic = "abort"`), so the host goes
  unreachable instead of serving a frozen result.

### D4 Performance: tiny bodies, ETags that hold, and nothing waits

- Serve stores the response **pre-serialized**: bytes plus `ETag` (a hash of the bytes), behind an
  `Arc` swapped under a mutex. It re-serializes only when a check's `status` or `message`
  changes, and `since` changes only then too, so the bytes and the ETag stay the same between
  changes. A request with a matching `If-None-Match` gets a `304` with no body. Every response
  carries `Cache-Control: no-cache`.
- The app keeps each host's last `ETag` and body in memory and sends `If-None-Match`. Most
  polls are a 304 of a few bytes. The config fetch works the same way (D2).
- The app's refresh runs every probe and every host fetch concurrently in one task group.
  The refresh takes as long as the slowest timeout (at most 30 s by the schema). In a `BGAppRefreshTask`, every timeout is capped at 10 s.
- The CLI uses threads, with no async runtime: one thread per check (sleep `every_s`, run `sh -c` with
  a kill timeout) and a small blocking HTTP server. Candidate crates are `tiny_http` (server) and `ureq`
  with rustls (client, only for a URL config). Both are small and have no tokio dependency. They are
  listed here so this approval covers them. Each one is added by the issue that first needs it,
  and M0 needs only `serde` and `serde_json`.
- Serve re-reads its config every 300 s (a conditional GET for a URL, the mtime for a path).
  An invalid new config is logged and the old one kept. A changed check list restarts only
  the threads whose `{id, command, every_s, timeout_s}` changed.
- **Resource budget (q2448, q2462, #38).** This is the standing limit for every later PR. `cli/tests/budget.rs`
  is part of `cargo test`, so CI's `rust` job fails when serve goes over it. Serve runs as a child process
  against a config with 5 remote hosts, a git checkout and two checks. `BEACON_SPEEDUP=60` divides every
  serve interval (the reload tick, the sampler, the git pull, the checks), so the 10 s run stands for
  600 s, and the readings are scaled back to real time:
  - CPU: at most 0.2% of one core. This is serve plus the processes it waits for, read from `/proc/<pid>/stat`
    on Linux or `beacon_sys::proc_usage` on macOS. It includes the test's own `true` checks, so it is an
    upper bound.
  - Memory: serve's peak resident set is at most 10 MB (Linux `VmHWM`; on macOS the peak footprint).
  - Network: an unchanged poll of `/status`, `/metrics` or `/config` is a 304 of at most 1 KB both ways.
    The fake hosts count the bytes of any poll serve makes.
  - Wake-ups: serve's own threads wake at most 70 times in an idle minute, a minute with no check due:
    the reload tick, the sampler and the git pull. On Linux these are the voluntary context switches of its
    live threads; on macOS, the kernel's wake-up counters. The processes it starts are not counted, and
    the count starts 1 s after the start. Each check adds about 5 a minute on Linux (spawn, wait, `kill`).
  - Logs: at most 512 KB a day. State: what serve writes under its config folder, `HOME` and `TMPDIR`
    grows at most 20 MB a day.
  - Latency: the p99 of unchanged polls on a kept-alive connection is at most 2 ms. The best of 5 rounds
    of 300 counts, so one scheduling spike on a busy host does not fail the gate.
  - `cargo test --features over-budget` with `BEACON_OVER_BUDGET=cpu|mem|wake|log|state|poll|slow`
    breaks one line on purpose: a busy loop, 20 MB, a 1 ms sleep loop, a log line a tick, a 21 MB file,
    no 304s or 3 ms a request. The test for that line must then fail.

### D5 Status semantics

- `http`: up when the response status is `expect_status` (absent: any 2xx) and, if `keyword` is set,
  the body contains it. Down on a timeout or an error.
- `tcp`: up when the connect completes within `timeout_s`.
- `heartbeat`: as in D3.
- Host check: up on exit 0, else down. `message` is the last non-empty line of stdout+stderr,
  cut to 200 chars. A check that runs past `timeout_s` is killed (its whole process group) and reported down, with the message "timeout after <n>s".
- The phone has no network path (`NWPathMonitor` unsatisfied): every row is **unknown**, not
  down, so the phone's own outage never shows as every service down. `worst` ignores unknown rows.

### D6 Secrets

The only secret is the optional config header: in the app's Keychain
(`kSecAttrAccessibleAfterFirstUnlock`, so background refresh can read it), and on the host
in `$BEACON_CONFIG_HEADER`. The widget never reads the Keychain (D8).

### D7 Module split: `cli/` is the Rust crate `beacon`; `app/` is the Xcode project

The schemas are shared as documents (`monitors.schema.json`, `status.schema.json`), not as code.
Each side writes its own types by hand. Both sides' tests decode `docs/monitors.example.json`, and
the CLI's test also round-trips a serve response against `status.schema.json`'s shape.
Boundaries are in `modules.md`.

### D8 The widget reads a snapshot; the app runs the checks

The app runs all checks (in the foreground every 60 s, on pull-to-refresh, and in `BGAppRefreshTask`)
and saves a `StatusSnapshot` in the App Group container. The widget extension only loads that file,
because a widget's time budget is too small for N network calls.

### D9 The owner supervises the CLI; the CLI installs nothing

`beacon serve` is a long-running foreground process, and the owner runs it under launchd or
systemd (a sample plist is in `usage.md`; nothing is installed by the CLI). `beacon run`
runs the host's checks once and prints them. It is for trying a command out and for CI.

### D10 Host metrics: serve samples its own host, the app reads `/metrics` (#13)

The question: the owner wants host load, cpu, memory and disk history with charts, in Beacon and not on
Drover's page. How does serve read them, what does it send, and how does the app find them?

Decided by the owner already (t127 q1742, q1746) and not reopened: one sample every 60 s of load 1/5/15, cpu %,
memory and swap used/total, and disk used/free per configured mount; the last 1440 samples in memory and
nothing on disk; `GET /metrics` as one JSON with an ETag, cached like `/status`; no Drover and no shared host.
The usage, shapes, reads and statecharts are in `metrics.md`, and the wire contract is `metrics.schema.json`.

What this ADR decides:

- **The sampler reads the OS's own text and parses it with pure functions. No new crate.** Linux: three
  `/proc` files. macOS: `sysctl` and `vm_stat`. Both: `df -kP`. The one value macOS offers no text for is
  cumulative cpu ticks, so `sampler.rs` makes one Mach call (`host_statistics`, which fills four integers).
  Options considered:
  - The `sysinfo` crate. Rejected: a new dependency larger than the rest of the tree, and it hides the reads
    the AC wants tested with fixtures.
  - Hand-written FFI for everything (`getloadavg`, `statvfs`, `sysctlbyname`). Rejected: struct layouts
    written by hand per OS, and a path that only the OS it runs on can test. Text parsers are tested on both.
  - `top -l 1` for macOS cpu. Rejected by measurement on the Mini: its first sample read 0–6 % idle while
    `iostat` and `top`'s second sample read 28–65 %. Those two are right, but they measure a single second,
    which is noisy, and take over a second to run.
    The Mach call matched `iostat` over the same 5 s (35.8 % against 35 %), and a difference of ticks gives
    the mean of the whole minute, which is what `/proc/stat` gives on Linux.
- **`sampler` sits beside `probe` and starts no process itself.** It reads files and makes the Mach call.
  For the tools it calls `probe::output(program, args, timeout)`, a new entry on probe's existing
  spawn-wait-kill helper, so `probe.rs` stays the only module that starts processes, and a hung `df` is
  killed after 5 s. The ring and the response live in `metrics.rs`, pure like `status.rs`.
- **The body is columns, not rows.** One array per series and one `t` array. A full day with one mount is
  about 115 KB, against about 285 KB as an array of objects; each further mount adds about 37 KB. The arrays
  are also what a chart takes. Rejected: gzip or a binary form (more moving parts than the size is worth),
  and a `?since=` query (the owner decided one JSON; the ETag already makes the polls between samples a 304).
- **A read that fails is a `null` in that sample, and each sample carries its own time.** So a missing mount,
  a hung tool or a sleeping host shows as a gap in a chart and never stops serve. `cpu_pct` is `null` in the
  first sample after a start, because it needs two readings.
- **"Used" is what the owner would call used.** Memory: what cannot be freed without swapping, so file cache
  is not counted (Linux `MemTotal − MemAvailable`; macOS as Activity Monitor's "Memory Used"). Disk: the
  volume's size minus what is available, so used + free is the size. On a Mac that makes `/` right without
  configuration; `df`'s own Used column would show only the 13 GiB system volume.
- **Mounts are `hosts.<name>.disks`**, an optional list of absolute paths, at most 8, default `["/"]`. It is a
  new field, so by D1 it is no version bump: update the CLI, then the config. The app ignores it. There is no
  switch for sampling itself and none for the interval.
- **The app derives the metrics URL from `hosts.<name>.url`**: the same URL with its last path segment
  replaced by `metrics`. A 200 means charts and a 404 means this host's serve has none. Rejected: a
  `metrics_url` field (the owner chose the existing URL, and it is one more thing to mistype), and a flag in
  the `/status` body (it changes a shape the shipped app reads, to say what a 404 says).
- **`/metrics` is cached by the mechanism of D4.** The sampler thread renders the bytes and the ETag once per
  sample and swaps them into a cell, and a request clones the cell. `/status` and `/metrics` are two cells
  of one type behind one request arm. A request never waits on a read.
- **The ring is bounded by count** (1440), in a type that cannot hold more. It is owned by the sampler
  thread alone, so it needs no lock. Samples are taken on a fixed 60 s schedule from the start of serve,
  the first one at once, so `curl` shows a point right after a start.
- **Serve itself is slice 1.** At b97cdf2 the CLI has `check` and `run` only: D3 and D4 were approved but
  `serve.rs` was never built. This issue builds the part of them it stands on: the listener (`tiny_http`,
  approved in D4), `/status` with ETag and 304, one thread per check, 404 for other paths and
  `panic = "abort"`. It does not build the 300 s config reload or a URL config (`source.rs`): serve takes a
  path here, as `run` does today, and those stay for the issue that needs them.

Consequences of D10:

- Anyone on the tailnet can read the host's load, memory and disk sizes, as they can read its checks (D3).
- The crate gets its first `unsafe`: the one Mach call, compiled on macOS only.
- The contract is now three schema documents and two shared examples (`docs/monitors.example.json`,
  `docs/metrics.example.json`). `/status` and `status.schema.json` do not change.
- The app fetches `/metrics` only while a host's detail screen is open (#14). The refresh, the background
  task and the widget stay as they are.
- A host with `"checks": []` serves metrics, but the app reaches a host through a heartbeat row. Whether a
  host without checks gets a row is #14's design question.
- About 450 non-test lines in `cli/src` (serve core about 170, metrics about 280). The cap is 2000 and the
  tree is at 1902, so impl needs a raise to about 2400.

## Consequences (D1–D9)

- No GitHub dependency anywhere. A new host means adding `hosts.<name>` and running serve there.
- An unreachable host turns its heartbeat rows red. That is intended (owner, q1516).
- Serve restarts show "no result yet" (down) for at most one check run. Accepted.
- A plain `http://` serve URL needs an ATS exception in the app. The recommended setup avoids
  that with `tailscale serve` (D3), so the app ships with `NSAllowsLocalNetworking` only.
- Repo layout for impl: `cli/` (crate), `app/` (`Beacon.xcodeproj`, `BeaconKit` package),
  `docs/`, `scripts/line-cap.sh`, `ci/cap.toml`, `.github/workflows/ci.yml`,
  `.herdsman/rules.md`. The line cap counts non-test lines in `cli/src` and in `app/**/*.swift`
  outside `*Tests*`.

## Hot files and `.drover/arch.toml`

D10 converts no hot file, so the impl lane adds no `.drover/arch.toml` lines. The new code is two new files
(`sampler.rs`, `metrics.rs`) and one file M0 named (`serve.rs`). The metrics docs are their own file
(`metrics.md`), so a later change to metrics does not edit the files the rest of the architecture shares.
