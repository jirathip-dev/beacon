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
