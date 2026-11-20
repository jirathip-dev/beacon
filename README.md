# beacon
Beacon: standalone iPhone uptime monitor + host heartbeat CLI

## CLI

Install: `cargo install --git https://github.com/jirathip-dev/beacon beacon`

- `beacon check <path>`: validate a `monitors.json` (exit 0 valid, 2 invalid with the reason on stderr).
- `beacon run [--config <path>] [--host <name>]`: run this host's checks (`hosts.<name>.checks`) once,
  concurrently, and print `id  status  "message"` per check. The config defaults to `$BEACON_CONFIG`, the
  host to the machine's short hostname, lowercased. Each check is `sh -c <command>`: up on exit 0, else down;
  past its `timeout_s` its whole process group is killed and it reports `timeout after <n>s`. Exit 0 all up,
  1 a check down, 2 config or usage error.
```sh
$ beacon run --config docs/monitors.example.json --host macmini
backup  down  ""
disk    up    "11% used"
```

- `beacon serve [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]` (default `127.0.0.1:7878`): runs this
  host's checks and serves JSON with an `ETag` (a matching `If-None-Match` gets `304`): `/status` (the checks),
  `/metrics` (24 h of load, cpu, memory and disk) and `/config` (the `--config` file's bytes, so the app fetches
  it over the tailnet with no token). The file is re-read when it changes; a change that is not a valid config
  for this host is logged and `/config` and the checks keep the last valid one. When the config's folder is a
  git checkout, serve runs `git pull --ff-only` in it at start and every 5 minutes; a failed pull is logged.
  New or removed checks take effect without a restart.
  With `--notify <path>`, serve also watches its own checks and every other host in `hosts` (it polls each
  host's `url` every 60 s) and POSTs signed webhooks for changes: a check that is up or down on 2 polls in a
  row, or a host that failed 3 polls in a row (and its first good poll after that). Changes within 30 s of the
  first one go out as one batch (`{"kind":"batch","summary":…,"changes":[…],"at":…}`); a down host's checks
  are left out while it is down. Something that stays down is reminded (`still down: <host>/<check> for <n>h`)
  every `notify.remind_every_h` hours, at most `notify.remind_max` times (top-level `"notify":
  {"remind_every_h": 4, "remind_max": 3}`, these defaults when absent; `remind_max: 0` turns reminders off).
  Nothing is sent at start or on a reload.
  Metric rules in `hosts.<name>.rules`, `{"id", "metric": "disk"|"memory"|"load", "above": <n>, "for_min": <n>}`,
  act like checks in `/status`: down once the metric (disk or memory % used per mount in `disks`, load1 per
  core) stays above `above` for `for_min` minutes, up once it stays at or below it as long. Without `rules`:
  `disk-full` > 90 for 10 min, `memory-full` > 90 for 10 min, `load-high` > 2 per core for 15 min; `"rules": []`
  turns them off. The notify file is `{"url": "https://…/hooks/in/<id>", "secret": "…"}`, conventionally
  `~/.config/beacon/notify.json`, mode 600 (serve exits 2 on a file group or others can read, a missing file or
  an invalid one). Each POST carries `X-Signature-256: sha256=<hex HMAC-SHA256 of the body under the secret>`.
  The secret is never logged. `SSL_CERT_FILE`, when set, replaces the built-in roots for these requests.
  Serve also lists its user's launchd (`gui/<uid>`, not `com.apple.*`) or `systemd --user` jobs every 60 s and
  reports each as a `job-<label>` row, judged by its kind; a job the config's `jobs` block does not name is
  shown in the app under Unreviewed, and with `--notify` sends one `new job found: <label>` webhook.

- `beacon jobs`: this host's jobs and their review; `beacon jobs keep|ignore|removed <label>` edits the
  config's `jobs` block and prints the `git commit` to run. Reference: [docs/jobs.md](docs/jobs.md).

- `beacon install [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]`: on macOS writes
  `~/Library/LaunchAgents/dev.beacon.serve.plist` running `beacon serve` with those flags (absolute paths,
  `RunAtLoad` + `KeepAlive`, log `~/.local/state/beacon/serve.log`) and loads it with `launchctl bootstrap`;
  on Linux writes the systemd user unit `~/.config/systemd/user/beacon-serve.service` (same flags, PATH and log,
  `Restart=always`) and runs `systemctl --user enable --now` (a changed unit: `restart`).
  Idempotent: `+ installed`, `~ updated` (rewritten and reloaded) or `= already` (nothing touched); exit 0.

Mini setup: copy the `jirathips-mac-mini` host block (its `doctor`, `clis` and `disk` checks and its `jobs`
block of kept launchd jobs) and its `Mini` monitors from `docs/hosts/mini.json` into your config, then run
`beacon install --config <that config>`. New jobs show up under Unreviewed; review them with `beacon jobs`. For alerts, make a webhook routine in a
cooee chat, put its URL and secret in `~/.config/beacon/notify.json` (`chmod 600` it) and run
`beacon install --config <that config> --notify ~/.config/beacon/notify.json` on the Mini only.

Boat VMs (boat-dev-1..4): copy the `boat-dev-1` host block and its `Boat` monitors from `docs/hosts/boat.json`
once per VM (rename the host and url), then run `beacon install --config <that config>` on that VM. Run
`loginctl enable-linger` once so the user unit runs without a login.
