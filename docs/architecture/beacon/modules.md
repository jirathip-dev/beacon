# Module boundaries

```
beacon/
  cli/                 Rust crate `beacon` (bin). Nothing here is used by app/.
    src/main.rs        clap: run | check. Exit 0 all up · 1 a check down · 2 config/network error.
    src/config.rs      MonitorsConfig parse + cross-file rules (types.md). No I/O.
    src/beat.rs        Beat wire type, serialisation. No I/O.
    src/state.rs       local State load/save (atomic rename). Only file I/O here.
    src/github.rs      Contents API: get(path, ref) with ETag, put(path, ref, sha). Only network here.
    src/run.rs         orchestrates: host checks → decide (statechart.md) → github.put → state.save
    src/check.rs       runs one host command with timeout, returns (Status, message)
  app/
    Beacon.xcodeproj
    Beacon/            SwiftUI app target: screens, Keychain, BGAppRefreshTask, writes StatusSnapshot
    BeaconWidget/      WidgetKit extension: reads StatusSnapshot only
    BeaconKit/         Swift package: config types, checks (http, tcp, heartbeat), GitHubReader, StatusSnapshot
  docs/                this ADR, monitors.example.json, cap-history/
  scripts/line-cap.sh  ci/cap.toml  .github/workflows/ci.yml  .herdsman/rules.md
```

Who may call whom (an arrow means "may import"):

```
cli:  main → run → {config, check, github, state, beat};  run is the only module that touches more than one of them
      config, beat: pure.  state: files only.  github: network only.  check: process only.

app:  Beacon → BeaconKit         BeaconWidget → BeaconKit (StatusSnapshot only)
      BeaconKit → Foundation, Network (NWConnection for tcp), Security (Keychain)
      BeaconWidget never imports the GitHub reader or the Keychain; Beacon never imports WidgetKit except to reload timelines.
```

Shared between cli and app: nothing but the two schema documents and `docs/monitors.example.json`,
which both test suites decode (ADR D7, acceptance test 4).
