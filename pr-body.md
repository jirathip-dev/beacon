Closes #13

Builds ADR D10 (`docs/architecture/beacon/metrics.md`), approved q1752.

- `serve.rs` (slice 1): `beacon serve [--config] [--host] [--listen 127.0.0.1:7878]`, tiny_http (approved in D4), `/status` and `/metrics` as two cells of one `Rendered` (bytes + ETag), 304 on `If-None-Match`, `Cache-Control: no-cache`, 404 elsewhere, one thread per check, one sampler thread on a fixed 60 s schedule (first sample at once, no burst after sleep), `panic = "abort"`.
- `sampler.rs`: Linux `/proc/{loadavg,stat,meminfo}`; macOS `sysctl -n vm.loadavg hw.memsize vm.swapusage`, `vm_stat`, one `host_statistics` Mach call; `/bin/df -kP -- <mount>` per mount, through new `probe::output` (5 s kill timeout). Pure parsers compiled on every OS.
- `metrics.rs`: `Ring` (at most 1440), column render per `metrics.schema.json`.
- `config.rs`: optional `hosts.<name>.disks` (`MountPath`), default `["/"]`, at most 8 distinct.
- `docs/metrics.example.json`: golden render.

Decided: no stderr log on a read starting/stopping to fail (statechart note) — kept out to save lines; a failed read shows as `null`. No `.drover/arch.toml` lines (ADR: none).

## Evidence at a6b86126837bfc66aeeca0e0bc911420cdeb4590

Merged `origin/staging` (92fc5f1, #17 raised line_cap to 3000) into the branch.

- `cd cli && cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test` → fmt exit 0, clippy exit 0, test exit 0 (config 6, metrics 5, run 6, sampler 5, serve 2 passed)
- `sh scripts/line-cap.sh` → exit 0: 2440 / 3000
- `sh scripts/line-cap.test.sh` → exit 0
- Proof on the Mini (run at fddffaf, src unchanged since): `beacon serve --host mini --listen 127.0.0.1:$DROVER_PORT`, after ~2 min `curl -si .../metrics` → 200, `ETag: "649f4c0aeac2332d"`, `t` with 3 points (`cpu_pct` [null, 99.8, 100.0], load1 87.11…); same ETag in `If-None-Match` → `304 Not Modified`.

src/ net +538 (budget ~+450): serve itself had to be built (slice 1) and rustfmt expands struct literals.

## For humans
The Beacon host service now records its own machine's load, CPU, memory, swap and disk use once a minute, keeping the last 24 hours in memory.
Anyone who can reach the host can fetch that history at the metrics address next to the existing status one; a restart starts the history empty.
