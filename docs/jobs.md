# `beacon jobs`

`beacon serve` discovers its user's jobs every 60 s: launchd `gui/<uid>` on macOS (not `com.apple.*`, `application.*` or
`dev.beacon.serve`), the `.service` and `.timer` units in `~/.config/systemd/user` on Linux (a timer and its
service are one job, under the timer's name; not `beacon-serve.service`). Each is a `/status` row
`job-<slug of its label>`. Beacon only watches: it never writes, loads or restarts a job. Design: ADR D12 in
`docs/architecture/jobs/`.

## Up or down

| kind (read from the job) | down when |
|---|---|
| KeepAlive (launchd `keepalive`; systemd `Restart=` not `no`) | it is not running |
| scheduled (`StartInterval`, `StartCalendarInterval`; a systemd timer) | its last exit failed, or 2 of its intervals passed with no run |
| on demand (anything else) | it is not running and its last exit failed |
| kept with `"running": true` | it is not running |
| kept | it is not loaded |

## The review: `hosts.<name>.jobs`

```json
"jobs": {
  "com.example.caffeinate": "ignore",
  "dev.example.old": "removed",
  "dev.example.tick": {"keep":{"name":"tick","group":"macmini"}}
}
```

- No entry: unreviewed. Shown in the app's Unreviewed group with its real status; with `--notify` one
  `new job found: <label>` webhook when it first appears (labels already seen are kept in
  `~/.local/state/beacon/jobs/<host>.json`; the first report after that file is missing sends nothing).
- `keep`: shown under that name and group; down when it vanishes.
- `ignore`: not shown, never alerts.
- `removed`: its removal is intended; not shown while it is gone, unreviewed if it comes back.

## Commands

`--config` defaults to `$BEACON_CONFIG`, `--host` to the short hostname. Exit 0, or 2 on a config or usage error.

- `beacon jobs [--config <path>] [--host <name>]`: one listing of this host, one line per job:
  `unreviewed|kept|ignored|removed  status  label  message  [name in group]`. A down job does not change the exit code.
- `beacon jobs keep <label> --name <n> --group <g> [--running]`
- `beacon jobs ignore <label>`
- `beacon jobs removed <label>`

Each edit rewrites only that host's `jobs` block (one label a line, sorted), leaves every other byte of the file
as it was, checks the result like `beacon check`, and prints `~ <host>: …` and the `git -C … commit … && git -C … push`
line to run; a second identical run prints `= <host>: …` and changes nothing. Serve re-reads the file when it
changes, and other hosts get it by their pull.

`$BEACON_LAUNCHCTL` or `$BEACON_SYSTEMCTL` names the tool and picks the manager (tests point it at a fake).
