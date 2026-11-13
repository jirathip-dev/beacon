# How the callers read

This was sketched before the code, so that the shapes serve the callers and not the other way around.

## An agent or the owner, editing the config wherever it lives

```sh
$ $EDITOR monitors.json && beacon check monitors.json          # or: beacon check https://gist.githubusercontent.com/…/raw/monitors.json
monitors.json: heartbeat "backup": host "macmini" has no check "backup"
$ echo $?
2                                                              # 0 = valid
```

## The host: `beacon serve`, supervised by launchd or systemd (the owner's job; D9)

```sh
$ BEACON_CONFIG=https://example.net/monitors.json beacon serve   # host = gethostname(); --host, --listen 127.0.0.1:7878 by default
serve macmini on 127.0.0.1:7878: 2 checks (backup every 300s, disk every 60s)
$ tailscale serve --bg 7878                                       # once: https://macmini.tail1234.ts.net/ → serve
$ curl -si https://macmini.tail1234.ts.net/status
HTTP/2 200
etag: "9f2c41d07ab3e815"
cache-control: no-cache
{"version":1,"host":"macmini","checks":{"backup":{"status":"up","message":"","since":"2026-11-12T03:00:04Z"},"disk":{"status":"up","message":"71% used","since":"2026-11-12T09:41:10Z"}}}
$ curl -si -H 'If-None-Match: "9f2c41d07ab3e815"' https://macmini.tail1234.ts.net/status
HTTP/2 304
```

A sample launchd `ProgramArguments`: `["/Users/me/.cargo/bin/beacon", "serve", "--config", "/Users/me/monitors.json"]`, with `KeepAlive` true.

```sh
$ beacon run --config ./monitors.json --host macmini    # one round, for trying a command out and for CI
backup  up    ""
disk    up    "71% used"
$ echo $?
0                                                        # 1 = a check is down, 2 = config error
```

## The app, Swift (`BeaconKit`)

```swift
let store  = ConfigStore(source: settings.source, header: try Keychain.header())   // .url(…) or .local
let config = try await store.load()                       // conditional GET; falls back to the cached copy
let snapshot = await Refresher(config: config, hosts: hostCache).run(now: .now)    // one task group:
//   http/tcp probes  +  one `GET hosts[h].url` per host (If-None-Match)  →  heartbeat rows mapped from its body
try snapshot.save()                                       // App Group container; the widget reads this
WidgetCenter.shared.reloadAllTimelines()
```

```swift
// Config screen: paste/import/edit becomes the local copy only if it decodes
do    { try store.saveLocal(text) }                       // throws the decode error, shown inline
catch { editor.error = error.localizedDescription }
```

```swift
// Widget timeline provider: no network, no checks
let snapshot = StatusSnapshot.load() ?? .empty
entries = [Entry(date: .now, worst: snapshot.worst, rows: snapshot.rows)]
```
