## Summary

Decision: `beacon serve` samples its own host once a minute (load, cpu, memory, swap, disk per mount), keeps the last 1440 samples in memory and serves them at `/metrics` as one JSON with an ETag, cached like `/status`. It reads the OS's own text plus one macOS system call for cpu, so there is no new dependency. The app finds metrics at the host's existing URL with `status` swapped for `metrics`; a 404 means the host has none.

Slices: (1) serve itself with `/status`, as M0 decided it; (2) the metrics shape, ring, schema and golden test; (3) the sampler with macOS and Linux fixtures; (4) the `/metrics` endpoint and the `disks` config field.

Open questions: (1) `beacon serve` is not built yet: the CLI has only `check` and `run`. I made it slice 1 of this issue; say so if you want it as its own issue first. (2) The line cap stands at 1902 of 2000 and this issue adds about 450 lines, so impl will ask for a raise to about 2400.

# Beacon architecture

**To review issue #13 (host metrics), read two things:** `metrics.md` (usage, shapes, what the sampler reads,
statecharts, tests) and D10 in `adr.md` (the decisions and the options turned down). The wire contract is
`metrics.schema.json`. The other files changed by a few lines each: `disks` in `monitors.schema.json` and
`types.md`, two modules in `modules.md`, `/metrics` in the serve chart of `statechart.md`.

M0 (D1–D9, approved): read `usage.md` (how it is used), `adr.md` (decisions), `types.md` plus
`monitors.schema.json` and `status.schema.json` (shapes), `modules.md` (boundaries), and `statechart.md`.
The canonical example is `docs/monitors.example.json`, which both test suites decode.

## Slices for #13, in commit order

1. **Serve core** (D3, D4, never built): `beacon serve [--config <path>] [--host <name>] [--listen <addr>]`,
   `tiny_http`, `/status` from a pre-rendered cell with ETag and 304, one thread per check, 404 elsewhere,
   `panic = "abort"`. No config reload and no URL config.
2. **Metrics shape**: `metrics.rs` (`Sample`, `Ring`, `render`), `docs/metrics.example.json`, the golden and
   ring tests.
3. **Sampler**: `sampler.rs` (parsers, reads, `Sampler`), `probe::output`, the fixtures and their tests.
4. **Wiring**: the sampler thread and `GET /metrics` in `serve.rs`, `hosts.<name>.disks` in `config.rs` and in
   `docs/monitors.example.json`, the serve test, the repo README, and the `curl` proof run on the Mini.
