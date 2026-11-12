# How the callers read

Sketched before the code, so the shapes serve the callers and not the reverse.

## The host, from cron or launchd (owner-scheduled, every 5 min)

```sh
$ beacon run                      # host name = gethostname(); config from beacon-config main
backup      up    "0 errors"      written (status changed)
disk        up    "71% used"      skipped (unchanged, written 12m ago)
$ echo $?
0                                 # 1 = a check is down, 2 = config/network error
```

```sh
$ beacon run --host macmini --config ./monitors.json --dry-run
```
`--config` reads a local file instead of the repo (tests, offline). `--dry-run` runs the
checks and prints the decision, writes nothing.

```sh
$ beacon check ./monitors.json    # validate only; exit 0 or 2 with one line per error
```

## The app, Swift (`BeaconKit`)

```swift
let github = GitHubReader(repo: "jirathip-dev/beacon-config", token: try Keychain.token())
let config = try MonitorsConfig(json: try await github.file("monitors.json", ref: "main"))
let beats  = try await github.beats()                      // [HeartbeatId: Beat], ref heartbeats

let results: [MonitorResult] = await withTaskGroup { group in
    for monitor in config.monitors { group.addTask { await monitor.check(beats: beats, now: .now) } }
    ...
}
try StatusSnapshot(results: results, at: .now).save()      // App Group container; the widget reads this
```

```swift
// Widget timeline provider: no network, no checks
let snapshot = StatusSnapshot.load() ?? .empty
entries = [Entry(date: .now, worst: snapshot.worst, rows: snapshot.rows)]
```

## The owner, once

```sh
gh api -X PUT repos/jirathip-dev/beacon-config/contents/beats/.keep \
  -f branch=heartbeats -f message='beats: init' -f content="$(printf '' | base64)"
```
(Needs the orphan branch to exist first: `git switch --orphan heartbeats && git commit --allow-empty -m init && git push -u origin heartbeats`.)
