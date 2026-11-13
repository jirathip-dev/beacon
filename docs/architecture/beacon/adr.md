# ADR: Beacon M0: config schema, host status, app/CLI split

Status: proposed (third round: q1511 and q1516 applied). Issue: jirathip-dev/beacon#1. Read `usage.md` first.

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

## Consequences

- No GitHub dependency anywhere. A new host means adding `hosts.<name>` and running serve there.
- An unreachable host turns its heartbeat rows red. That is intended (owner, q1516).
- Serve restarts show "no result yet" (down) for at most one check run. Accepted.
- A plain `http://` serve URL needs an ATS exception in the app. The recommended setup avoids
  that with `tailscale serve` (D3), so the app ships with `NSAllowsLocalNetworking` only.
- Repo layout for impl: `cli/` (crate), `app/` (`Beacon.xcodeproj`, `BeaconKit` package),
  `docs/`, `scripts/line-cap.sh`, `ci/cap.toml`, `.github/workflows/ci.yml`,
  `.herdsman/rules.md`. The line cap counts non-test lines in `cli/src` and in `app/**/*.swift`
  outside `*Tests*`.
