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

- `beacon install [--config <path>] [--host <name>] [--listen <addr>]`: on macOS writes
  `~/Library/LaunchAgents/dev.beacon.serve.plist` running `beacon serve` with those flags (absolute paths,
  `RunAtLoad` + `KeepAlive`, log `~/.local/state/beacon/serve.log`) and loads it with `launchctl bootstrap`;
  on Linux writes the systemd user unit `~/.config/systemd/user/beacon-serve.service` (same flags, PATH and log,
  `Restart=always`) and runs `systemctl --user enable --now` (a changed unit: `restart`).
  Idempotent: `+ installed`, `~ updated` (rewritten and reloaded) or `= already` (nothing touched); exit 0.

Mini setup: copy the `jirathips-mac-mini` host block and its `Mini` monitors from `docs/hosts/mini.json`
into your config, then run `beacon install --config <that config>`.

Boat VMs (boat-dev-1..4): copy the `boat-dev-1` host block and its `Boat` monitors from `docs/hosts/boat.json`
once per VM (rename the host and url), then run `beacon install --config <that config>` on that VM. Run
`loginctl enable-linger` once so the user unit runs without a login.
