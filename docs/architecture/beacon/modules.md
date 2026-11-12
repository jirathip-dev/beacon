# Module boundaries

```
beacon/
  cli/                 Rust crate `beacon` (bin). Nothing here is used by app/.
    src/main.rs        clap: serve | run | check. Exit 0 ok/all up · 1 a check down (run) · 2 config/bind error.
    src/config.rs      MonitorsConfig parse + cross-field rules (types.md). No I/O.
    src/source.rs      load config from path (mtime) or URL (ETag, header). Only outbound network here.
    src/probe.rs       run one `sh -c` command with a kill timeout → (Status, message). Only processes here.
    src/status.rs      StatusBody, record, render (bytes + ETag). Pure.
    src/serve.rs       HTTP server (GET /status, If-None-Match → 304), one scheduler thread per check, config reload.
    src/run.rs         one round of the host's checks, concurrently; print; exit code.
  app/
    Beacon.xcodeproj
    Beacon/            SwiftUI app target: list, config source screen (URL / import / paste / edit),
                       Keychain (header), BGAppRefreshTask, 60 s foreground timer, writes StatusSnapshot
    BeaconWidget/      WidgetKit extension: reads StatusSnapshot only
    BeaconKit/         Swift package: config + status types, ConfigStore (source, cache, ETag),
                       probes (http, tcp, host fetch with ETag), Refresher, StatusSnapshot
  docs/                this ADR, monitors.example.json, cap-history/
  scripts/line-cap.sh  ci/cap.toml  .github/workflows/ci.yml  .herdsman/rules.md
```

An arrow means "may import":

```
cli:  main → {serve, run, config, source}
      serve → {config, source, probe, status}      run → {config, source, probe, status}
      config, status: pure.  source: network/files only.  probe: processes only.
      serve and run share probe + status, so `beacon run` prints exactly what serve would report.

app:  Beacon → BeaconKit         BeaconWidget → BeaconKit (StatusSnapshot only)
      BeaconKit → Foundation, Network (NWConnection for tcp, NWPathMonitor)
      Keychain access lives in Beacon (the app target), which hands BeaconKit the header value.
      BeaconWidget never imports the probes or the Keychain. Beacon imports WidgetKit only to reload timelines.
```

The cli and the app share nothing except the two schema documents and `docs/monitors.example.json`,
which both test suites decode (ADR D7, acceptance test 4).
