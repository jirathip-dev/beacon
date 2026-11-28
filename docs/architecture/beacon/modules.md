# Module boundaries

```
beacon/
  cli/                 Rust crate `beacon` (bin). Nothing here is used by app/.
    src/main.rs        clap: serve | run | check. Exit 0 ok/all up · 1 a check down (run) · 2 config/bind error.
    src/config.rs      MonitorsConfig parse + cross-field rules (types.md). No I/O.
    src/source.rs      load config from path (mtime) or URL (ETag, header). Only outbound network here.
    src/probe.rs       run one `sh -c` command with a kill timeout → (Status, message); `output`: run one tool,
                       same kill timeout → its text (D10). Only processes here.
    src/sampler.rs     D10: read this host's load, cpu ticks, memory, swap and disks (files, one Mach call, tools
                       through probe::output) and parse them → Sample. Only OS reads here; it writes nothing.
    src/status.rs      StatusBody, record, render (bytes + ETag). Pure.
    src/metrics.rs     D10: Sample, Ring (at most 1440), render to columns (bytes + ETag). Pure.
    src/serve.rs       HTTP server (GET /status and /metrics, If-None-Match → 304), one scheduler thread per check,
                       one sampler thread, config reload.
    src/run.rs         one round of the host's checks, concurrently; print; exit code.
  app/
    Beacon.xcodeproj
    Beacon/            SwiftUI app target: list, config source screen (URL / import / paste / edit),
                       Keychain (header), BGAppRefreshTask, 60 s foreground timer, writes StatusSnapshot
    BeaconWidget/      WidgetKit extension: reads StatusSnapshot only
    BeaconKit/         Swift package: config + status types, ConfigStore (source, cache, ETag),
                       probes (http, tcp, host fetch with ETag), Refresher, StatusSnapshot
  docs/                this ADR, monitors.example.json
  .github/workflows/ci.yml  .herdsman/rules.md
```

An arrow means "may import":

```
cli:  main → {serve, run, config, source}
      serve → {config, source, probe, status, sampler, metrics}      run → {config, source, probe, status}
      sampler → {probe (output only), metrics (Sample and its parts), config (MountPath)}
      metrics → {status (Rendered), config (MountPath)}
      config, status, metrics: pure.  source: network/files only.  probe: processes only.  sampler: OS reads only.
      serve and run share probe + status, so `beacon run` prints exactly what serve would report.
      Only serve calls sampler and metrics: `run` and `check` do not sample.
      `source.rs` and serve's config reload are not built yet (D10, last point): until then main reads the path itself.

app:  Beacon → BeaconKit         BeaconWidget → BeaconKit (StatusSnapshot only)
      BeaconKit → Foundation, Network (NWConnection for tcp, NWPathMonitor)
      Keychain access lives in Beacon (the app target), which hands BeaconKit the header value.
      BeaconWidget never imports the probes or the Keychain. Beacon imports WidgetKit only to reload timelines.
```

The cli and the app share nothing except the three schema documents and the two examples
(`docs/monitors.example.json`, `docs/metrics.example.json`), which both test suites decode (ADR D7, D10).
