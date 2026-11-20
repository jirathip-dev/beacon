## Summary

Decision: The phone keeps probing the http/tcp monitors for the live row, and one host named in the config (`"prober"`, the Mini) probes them too, every 60 s, to keep their history. Every serve keeps the up/down changes of everything in its `/status` (checks, and #41's rules and #39's jobs with no work of theirs) in one small JSON file, and the prober keeps response times in one fixed 5.8 KB file per monitor: 24 h per probe plus 30 days per hour. The app reads `/history` (changes; the ETag holds until the next change) and `/history/<id>` (one monitor's times) only while a detail screen is open. A certificate that stops being valid within 14 days makes the monitor down on both sides.

Slices: (1) the history shapes, schemas and bound tests; (2) the store on disk, the tap in serve and `/history` for checks; (3) the prober: `prober` and `cert_warn_days` in the config, http/tcp/cert probes, the time rings, `/history/<id>`, the budget test with 20 monitors; (4) after design, the app: uptime, incident log, response-time chart, the certificate rule.

Open questions: (1) The row's colour stays the phone's own probe and the history is the Mini's view; say if the Mini should be the one truth instead (the app would stop probing). (2) The Mini does not send monitor changes to cooee in this issue; say if it should. (3) `rustls` becomes a direct dependency for the certificate check; it is already compiled in as ureq's TLS, so no new crate is downloaded.

# ADR: history, TLS expiry and the prober (issue #42)

Status: proposed. It amends D3, D4, D5, D7 and D8 of `../beacon/adr.md`, which stay as they are otherwise. The
decisions are numbered H1–H8 so they cannot clash with a `D` number another open issue takes. Read
`history.md` first: it has the callers' usage, the shapes, the statecharts and the numbers.

## Question

The owner decided (q2460): TLS-expiry warnings, uptime % with an incident log, and response-time charts per
http monitor, stored on the Mini. Today serve never probes an http or tcp monitor and writes nothing but its
log. So: who probes what, what is stored and how is it bounded, what does the app fetch, and how does all of
it stay inside #38's budget (CPU ≤ 0.2 % of a core, peak memory ≤ 10 MB, state folder ≤ 20 MB)?

## Decisions

### H1 Who probes what: both, for different questions

| | the phone app | every `beacon serve` | serve on the `prober` host |
|---|---|---|---|
| host checks (and #41's rules, #39's jobs) | reads `/status`, as today | runs them, keeps their up/down changes | the same |
| `http` and `tcp` monitors | probes them for the live row, as today (D3, D8) | nothing | probes each every 60 s; keeps changes and response times |
| history | reads it while a detail screen is open | serves `/history` | serves `/history` and `/history/<id>` |

The row answers "is it up from where I stand, now". The history answers "what did the Mini see over 30 days".
The two can differ for a minute, or for good on a target only one of them can reach.

`monitors.json` gets one optional top-level field, `"prober": "<host id>"`. Only that host's serve probes the
monitors. Absent: nobody does, and everything behaves as before this issue. The app reads the same field to
know which host to ask for a monitor's history. It is a new field, so by D1 it is no version bump.

Options considered:
- **The Mini is the one truth; the app stops probing.** One vantage point, fewer phone requests, and the
  Mini's notifier could cover monitors. Rejected for now: the issue says serve probes "as well", the app
  would show nothing useful with the Mini off or the tailnet down (D2: the app works with no host), and it
  changes what a shipped screen means. It is open question 1.
- **A `--probe` flag on serve**, like `--notify`. Rejected: the app could not know which host has the history
  without asking each one.
- **Every serve probes the monitors.** Rejected: five hosts would probe every target for no extra knowledge.

### H2 One tap for everything in `/status`

Serve gets one `record` function, the only writer of `/status`. It also feeds the history. So anything that
shows as a check in `/status` has history without knowing it: today's checks, #41's rules, #39's jobs. A
monitor's result from the prober goes into the same `History` under the monitor's id; `/status` is unchanged.

A change is stored as an incident: `down_at`, `up_at` (null while it is down) and the message it went down
with. A changed message during one incident is not stored, so a check that prints a new number every minute
writes nothing. The first result after a restart is compared with the stored state: the same state continues
the incident, a different one is a change at that moment.

### H3 The store: one JSON file of incidents, one fixed ring file per monitor

Under `~/.local/state/beacon/history/`:

- `events.json`: every key's incidents, in the exact shape of the `/history` body (one schema, one parser).
  It is rewritten whole (temporary file, then rename) on the existing 60 s tidy tick, and only when it
  changed. When it did not change, its modification time is set to now: that time is the "serve was alive"
  stamp.
- `times/<id>.bin`: one file per http/tcp monitor, 5 776 bytes for good. 1 440 slots of one probe each (24 h)
  and 720 slots of one hour each (30 d: average and maximum). A probe changes about 14 bytes in place.

Bounds, each by count in a type that cannot hold more (as D10's ring):

| what | bound | when full |
|---|---|---|
| incidents per key | 64, none older than 30 days | the oldest goes; the key's `since` moves to its end, so uptime stays right for what is kept |
| gaps (serve not watching) | 128, none older than 30 days | the oldest goes |
| message | 120 characters | cut |
| response times per monitor | 1 440 probes + 720 hours | the slot of the same minute or hour is overwritten |
| a ring file nobody wrote for 30 days | removed by the tidy tick | |

Options considered:
- **An append-only log** (`t key status` lines, samples too). Rejected: 20 monitors make 864 000 samples in 30
  days, about 21 MB as text, over the folder's cap; it needs compaction and a replay at start.
- **SQLite.** Rejected: a new dependency several times the size of the tree for two shapes.
- **Everything in one JSON rewritten every minute.** Rejected: about 240 KB written every minute (345 MB a
  day) to change 40 bytes.
- **Times in memory only, as `/metrics`.** Rejected: the owner asked for 30 days stored on the Mini, and
  every `beacon install` restart would blank the charts.
- **Daily down-seconds counters for uptime.** Rejected: a second shape. Incidents give uptime and the log.

Serve was not watching while it was stopped or the host slept. The tidy tick sees that as a jump of more
than 180 s since the alive stamp and stores it as a gap. A gap counts neither as up nor as down.

### H4 The endpoints: a small index that rarely changes, and one series per monitor

- `GET /history`: `history.schema.json`. `gaps`, `checks` (by id) and, on the prober, `monitors` (id, group,
  name), each with `since` and `down`. It holds no percentage and no "now", so its bytes and ETag change
  only when something goes down or up. The app works out uptime for any window from it.
- `GET /history/<id>`: `times.schema.json`. `recent` (`t`, `ms`: one entry per probe, 24 h) and `hourly`
  (`t`, `avg_ms`, `max_ms`: 30 d). Columns with explicit times, as `/metrics`, so the app's chart code takes
  them as it takes a metrics series. About 35 KB when full; its ETag changes once a minute.

Both are pre-rendered cells behind the request arm of D4: a request clones bytes and never waits. The app
finds them at the host's `url` with the last segment replaced, as it finds `/metrics`. A 404 means a serve
from before this issue.

A monitor's `id` is 16 hex digits, the FNV-1a 64 hash of `<group>/<name>`. Serve works it out and the index
publishes it; the app never hashes. It is the file name and the URL segment. With `prober` set, `beacon
check` refuses two http/tcp monitors with the same `<group>/<name>`. A renamed monitor starts a new history.

Rejected: one body with every monitor's times (about 700 KB for 20 monitors, new every minute); percentages
computed by serve (the body would change every minute and the app could show only the windows serve chose);
a `?since=` query (D10 turned it down for the same reason).

### H5 The certificate rule (amends D5)

An `http` monitor is down, with `cert expires in <n>d`, when its certificate chain stops being valid within
`cert_warn_days` (default 14, 0–365, 0 turns the early warning off). Both probers apply it:

- **Serve** makes one extra TLS handshake per monitor per hour with rustls's clock set `cert_warn_days`
  ahead. A chain that is still valid then passes. One that is not fails with the certificate's `notAfter`,
  and `n` is counted from the real clock on every probe until the hourly check passes again. Tried on the
  Mini: 14 days ahead passes on the test certificate; far ahead fails with its exact `notAfter`.
- **The app** reads the leaf's `notAfter` from the server trust of the request it already makes (iOS 18's
  `SecCertificateCopyNotValidAfterDate`; the app's minimum is iOS 18).

Rejected: reading the certificate from ureq's own connection (ureq 3 exposes neither the peer certificate
nor its rustls config); a DER parser crate (`x509-parser`: a new dependency tree to read one date); a
hand-written DER walk (untested parsing of hostile input).

### H6 The prober: four threads, one connection each, never a shared pool (amends D4)

Four prober threads share the monitors (`index % 4`). Each walks its monitors in turn and then sleeps to the
next 60 s mark. Every probe opens a new connection, so the time is what a new visitor waits: name lookup,
connect, TLS, request, up to the response's status line and headers. The body is read (at most 256 KB) only
when the monitor has a `keyword`. ureq's two buffers are set to 16 KB.

Why four and not one per monitor, measured on the Mini (ureq 3.5.2, rustls 0.23.46, a local TLS server):

| | default buffers (128 KB) | 16 KB buffers |
|---|---|---|
| probes one after another | 3.5 MB peak | 2.5 MB |
| 4 at once | 5.7 MB | 3.0 MB |
| 20 at once | 17.2 MB | 5.2 MB |

One thread per monitor reaches the memory limit exactly when it matters, with every target hanging. Four
threads bound the memory whatever the config holds. One thread would take 200 s for a round of 20 hanging
targets; four take 50 s. A thread whose round overruns starts the next at once.

### H7 The budget (#38)

Measured at 3616a81 on the Mini by `cargo test --test budget`: 0.412 s CPU in 600 s (0.069 % of a core),
2.49 MB peak, 45.6 wake-ups in an idle minute.

| line | limit | what this issue adds |
|---|---|---|
| CPU | 0.2 % | 2 000 fresh HTTPS probes cost 0.39 s CPU, so 0.2 ms each: 20 a minute are 0.007 % of a core. The hourly certificate handshakes are 20 more an hour. Rendering 20 series a minute is under 0.005 %. |
| memory | 10 MB | about +1.6 MB for the TLS client and four probes at once, +0.8 MB for 20 rings and their rendered bodies: about 5 MB. |
| wake-ups | 70 an idle minute | none with no monitors: the store rides the existing tidy tick, and prober threads exist only on the prober. A probe adds a few (its socket waits), as a check adds 5. |
| network | a 304 ≤ 1 KB | `/history` and each `/history/<id>` join the unchanged-poll test. |
| disk | 20 MB | 116 KB of rings for 20 monitors plus `events.json`: see H8. |
| locks | none across I/O | incidents are recorded under serve's existing lock with no I/O; the file is written after the lock is released; a ring is owned and written by its prober thread alone. |
| latency | p99 ≤ 2 ms | the new paths are the same cell clone. |

D4's line "serve's only disk writes are the rotation and the trim" becomes: those two, `events.json` on the
same thread, and each ring by its own prober thread. No request and no check ever waits on them.

The budget test's `idle` config gets `"prober": "box"` and 20 http monitors on a local TLS server (AC). One
new `BEACON_OVER_BUDGET` line, `history`, turns the bounds off (no incident cap, rings append), and the
size test must then fail.

### H8 Disk use for 30 days of 20 monitors

| file | size | note |
|---|---|---|
| `times/*.bin` | 20 × 5 776 B = 116 KB | the same on day 1 and day 30 |
| `events.json`, usual | under 20 KB | about 60 keys, a few incidents each |
| `events.json`, worst | about 650 KB | every one of 60 keys at 64 incidents with 120-character messages |
| total | about 140 KB, never over 1 MB | the folder's cap is 20 MB; a day of writes is about 0.5 MB |

## Consequences

- The Mini makes 20 outbound requests a minute, all day. A target sees one more visitor a minute.
- Uptime is "of the time the Mini was watching". Mini off for a day: that day is a gap, not downtime.
- A new check or monitor shows uptime since it was first seen, not 30 days. The app labels the real span.
- A check that flaps more than 64 times in 30 days shows a shorter span, never a wrong percentage.
- A monitor's row can be green on the phone while the Mini's log shows an incident, and the other way round.
- Anyone on the tailnet can read the incident log and response times, as they can read `/status` (D3).
- `rustls` moves from a dev-dependency to a dependency. `Cargo.lock` gains no crate.
- `notify.rs` loses its private `agent()` to a new `net.rs` that the prober shares.
- About +450 non-test lines in `cli/src` and about +350 in the app. The tree is at 3604 of 4500.
- Retiring Uptime Kuma stays the owner's later call; nothing here touches it.

## Hot files and `.drover/arch.toml`

This issue converts no hot file, so the impl lane adds no `.drover/arch.toml` lines. It is laid out so it
shares as little as it can with #39, #40 and #41, which are open on the same crate:

- Its docs are this folder, not `docs/architecture/beacon/`. Impl adds the two config fields to
  `../beacon/monitors.schema.json` (the fragment is in `history.md`) and nothing else there.
- Its code is new files: `history.rs`, `monitor.rs`, `prober.rs`, `net.rs`, and `History.swift` in BeaconKit.
  `serve.rs` gets the `record` function, one tick call, one request arm and two calls to `prober::run`.
