# `beacon serve`

`beacon serve [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]` (default `127.0.0.1:7878`): runs this
host's checks and serves JSON with an `ETag` (a matching `If-None-Match` gets `304`): `/status` (the checks),
`/metrics` (24 h of load, cpu, memory and disk) and `/config` (the `--config` file's bytes, so the app fetches
it over the tailnet with no token). The file is re-read when it changes; a change that is not a valid config
for this host is logged and `/config` and the checks keep the last valid one. When the config's folder is a
git checkout, serve runs `git pull --ff-only` in it at start and every 5 minutes; a failed pull is logged.
New or removed checks take effect without a restart.

Alerts with `--notify`: [alerts.md](alerts.md). Metric rules: [rules.md](rules.md). Jobs: [jobs.md](jobs.md).
