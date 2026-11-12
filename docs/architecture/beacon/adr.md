# ADR: Beacon M0 — config schema, heartbeats, app/CLI split

Status: proposed. Issue: jirathip-dev/beacon#1. Read `usage.md` first.

## Question

Beacon is an iPhone app that checks things itself (pull) and a host CLI that reports
status (push), with nothing hosted: a private GitHub repo `beacon-config` is the only
shared state. What are the config schema, the heartbeat format, the write policy, the
auth model and the module split, so every later issue builds on the same shapes?

## Decisions

### D1 `monitors.json` on `beacon-config` `main`, `version: 1`, strict

One file, one integer `version`. Shapes in `monitors.schema.json`. Readers reject a
version they do not know and reject unknown fields. Monitor types are a tagged union on
`type`: `http`, `tcp`, `heartbeat`. Host checks live beside them under `hosts`, keyed by
host name; each host check `id` must be the `id` of a `heartbeat` monitor.

Options considered:
- Ignore unknown fields (forward compatible). Rejected: a typo (`timeout` for
  `timeout_s`) would silently take the default; with one owner and two readers, updating
  both before using a new field is cheap. Adding a field is a version bump.
- One file per monitor type or per host. Rejected: more fetches, nothing gained at this size.

### D2 Heartbeats: one file per id on the orphan branch `heartbeats`

`beats/<id>.json`, shape in `heartbeat.schema.json`: `{id, status, message, host, time}`.
`status` is `up|down`, `time` ISO-8601 UTC. The branch is an orphan (created once by the
owner, `usage.md`), so config history and beat history never conflict.

Options considered:
- One aggregate `beats.json`. Rejected: two hosts writing at once conflict on one file.
- Per-id files (chosen): writers never conflict; the app lists `beats/` once and fetches
  each file. Cost O(N) requests per refresh; fine to ~50 heartbeats, and conditional
  requests (ETag, 304) are free against the rate limit. Revisit with a tree read if N grows.

### D3 The CLI writes through the GitHub Contents API, no clone

`beacon run` runs its host's checks, then for each beat the write policy (D4) decides;
a write is one `PUT /repos/{o}/{r}/contents/beats/<id>.json` on `heartbeats` with the
file's last `sha` from local state. A 409 (stale sha) re-reads the sha and retries once.
`monitors.json` is fetched from `main` the same way each run, with its ETag cached.

Options considered:
- A local clone and `git push`. Rejected: a clone per host to keep in sync, merge
  trouble on a shared branch, more host state.
- Git Data API batching all beats in one commit. Rejected for M0: more code; one commit
  per changed beat is few commits by D4.

### D4 Write policy: on change, else every 30 minutes

Local state at `$XDG_STATE_HOME/beacon/state.json` (default `~/.local/state/beacon/`)
holds, per id, the last written `status`, `written_at` and the file `sha`. Write when the
status differs from the last written one, or `now - written_at >= 30 min`, or no record
exists. A failed write leaves the record unchanged, so the next run retries. See
`statechart.md`.

Consequence for the config: with `beacon run` every 5 min, an unchanged `up` is
rewritten within 35 min, so a heartbeat's `max_age_min` must be above that; the
example uses 45. The CLI's `check` warns under 40.

### D5 Status semantics

- `http`: up when the status is `expect_status` (absent: any 2xx) and, if `keyword` is
  set, the body contains it; down on timeout or error.
- `tcp`: up when the connect completes within `timeout_s`.
- `heartbeat`: down when the latest beat is `down`, when `time` is older than
  `max_age_min`, or when there is no beat file yet; else up.
- Host check: exit 0 = up, else down; `message` = the last non-empty line of
  stdout+stderr, cut to 200 chars; a check past `timeout_s` is killed and down.

### D6 Auth

App: a fine-grained PAT with Contents: read on `beacon-config` only, in the Keychain
(`kSecAttrAccessibleAfterFirstUnlock` so background refresh can read it). The widget
never touches the token (D8). CLI: `BEACON_GITHUB_TOKEN`, else `gh auth token`; no new
secret on the host.

### D7 Module split: `cli/` Rust crate `beacon`, `app/` Xcode project

Schema shared by document (`monitors.schema.json`, `heartbeat.schema.json`), not code:
each side hand-writes its types; each side's tests decode `docs/monitors.example.json`.
Boundaries in `modules.md`.

### D8 The widget reads a snapshot; the app runs the checks

The app (foreground and `BGAppRefreshTask`) runs all checks and saves a `StatusSnapshot`
in the App Group container. The widget extension only loads it. A widget's runtime
budget is too small for N network checks, and this keeps the token out of the extension.

### D9 Scheduling is the owner's

`beacon run` is one-shot. The owner schedules it (cron, launchd); the CLI installs nothing.

## Consequences

- Two readers to update on any schema change (strict readers, D1). Accepted.
- Beats are visible only on refresh: the app shows `time` so staleness is honest.
- A new heartbeat monitor is red until its first beat (D5). Accepted: that is the point.
- Repo layout for impl: `cli/` (crate), `app/` (`Beacon.xcodeproj`, `BeaconKit` package),
  `docs/`, `scripts/line-cap.sh`, `ci/cap.toml`, `.github/workflows/ci.yml`,
  `.herdsman/rules.md`. The line cap counts non-test lines in `cli/src` and `app/**/*.swift`
  outside `*Tests*`.
