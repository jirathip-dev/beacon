# `beacon install`

`beacon install [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]`: on macOS writes
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
