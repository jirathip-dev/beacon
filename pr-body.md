Closes #25

## For humans
Beacon can now install itself as a background service on the Linux boat VMs too, so its status server restarts after a crash or reboot. Running the install again changes nothing unless the settings changed.
A ready-made boat VM config adds health checks for the CI runner service, the CLIs that lanes need, and disk space; copy it once per VM.

## What changed
- `cli/src/install.rs`: one `install` for both platforms (shared home/exe/config/log/PATH); Linux writes `~/.config/systemd/user/beacon-serve.service`, then `systemctl --user daemon-reload` + `enable --now` (new) or `restart` (changed); unchanged unit calls nothing. `BEACON_SYSTEMCTL` injects the runner, like `BEACON_LAUNCHCTL`.
- `cli/src/main.rs`: drops the "macOS only" refusal.
- `docs/hosts/boat.json`: `boat-dev-1` host template with `runner` (any `actions.runner.*` service active), `clis` (drover shed claude gh git), `disk` (< 90%) and their `Boat` heartbeat monitors.
- `cli/tests/install.rs`: Linux test (fake systemctl): first run writes + enables, second run makes no calls and leaves the file, changed `--listen` rewrites + restarts. Mini/boat config tests share one helper.

## Evidence (head 10dfcbdb884bc2ce39b77151f969f6f4c80ca71e, macOS; staging merged in, incl. the cap raise #28)
- `sh scripts/line-cap.sh` → exit 0 (non-test lines: 3026 / 3526)
- `cd cli && cargo fmt --check` → exit 0
- `cd cli && cargo clippy --all-targets -- -D warnings` → exit 0
- `cd cli && cargo test` → exit 0 (macOS launchd test, mini + boat config tests)
- `beacon check docs/hosts/boat.json` → exit 0
- The Linux systemd test runs in CI's ubuntu `rust` job (it passed at the previous head 5c5d2b16685e4ba95a65de4d319b88dc60d806a1).

## Line cap
src/ net +27 (budget not stated): a second service manager needs its own unit text and steps; the shared setup was folded in to keep it small. Fits under the raised cap (3026 / 3526, owner decision q1839, raised in #28).
