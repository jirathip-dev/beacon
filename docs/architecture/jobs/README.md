## Summary

Decision: `beacon serve` lists its user's launchd or systemd jobs once a minute and puts each in `/status` as a row that already carries its name and group. A row with no group is unreviewed, and the app shows it under Unreviewed; the app never reads the new `jobs` block. Up or down follows the job's own kind. `beacon jobs keep|ignore|removed` rewrites only one host's `jobs` block and prints the commit to run.

Slices: (1) the `jobs` block in the config and schema; (2) discovery on macOS, the down rule and the `/status` rows; (3) the `new job found` webhook and the budget test with 50 fake jobs; (4) `beacon jobs`; (5) discovery on Linux; (6) the app's rows and Unreviewed group; (7) the Mini's migration and the README.

Open questions: (1) Should an unreviewed job that vanishes just leave the list? I made only kept jobs go down on vanishing, because drover's one-shot wake jobs would each page you and stay red. (2) The first start finds 15 unreviewed jobs on the Mini. I send no webhook for that first list (they show in the app). Send all 15 instead? (3) The boat's runners are system units, which `systemd --user` cannot see, so `runner` stays a shell check. Or should Linux also read system units? (4) The Mini's two Actions runners are not KeepAlive, so they go down only on a failed exit, not whenever they are not running as today. Accept, or add an optional "must be running" mark on a kept job?

# Job discovery: architecture for issue #39

Read `design.md` first: how the owner, serve, the notifier and the app use it, then the shapes, what is read
from launchd and systemd, the statecharts, the module boundaries and the tests. Then `adr.md` (D12): the ten
decisions, the options turned down and the consequences. `schema-additions.json` is what the two contract
schemas in `../beacon/` gain. D1–D11 are in `../beacon/adr.md` and are not changed.

## Slices, in commit order

1. **Config**: `Label`, `Review` and `hosts.<name>.jobs` in `config.rs`; the `job-` prefix reserved; the
   additions folded into `monitors.schema.json`; a `jobs` block in `docs/monitors.example.json`.
2. **Discovery on macOS**: `jobs.rs` (the rule, `Watcher`, `Manager`, `watch`), `jobs/launchd.rs`, the
   launchd fixtures and the fake tool; the thread in `serve.rs`; `name` and `group` in `status.rs` and
   `status.schema.json`. Every test that starts serve names the fake tool.
3. **Announcing**: `state::Announced` and the trim that spares `jobs/`; `Event::job` for serve's own host and
   for polled hosts; the one-webhook test; `budget.rs` with 50 fake jobs.
4. **`beacon jobs`**: `jobs/cli.rs`, `jobs/edit.rs`, the round-trip test, `docs/jobs.md`.
5. **Discovery on Linux**: fixtures captured on a boat, then `jobs/systemd.rs`.
6. **App**: `name` and `group` on `CheckResult`, `host` on `MonitorResult`, the job rows in `Refresher`,
   `Model.host(of:)` removed; the verify fixture, recipe and 390 shot.
7. **Migration**: `docs/hosts/mini.json` and the README's setup lines.

Slices 1 to 4 and 7 make the Mini whole. Slice 5 can land later without changing them: on Linux, serve
before slice 5 finds no jobs.

## What was checked on the Mini for this design (2026-11-20, read-only)

- `launchctl list` shows 24 jobs of this user besides Apple's and Beacon's own: 9 are today's hand checks,
  15 are watched by nothing. `dev.drover.t135-wake`, in the issue's list, was already gone.
- The kinds, from `launchctl print`: 8 KeepAlive, 7 with an interval (60 s to 1800 s), 1 calendar
  (`dev.drover.clean`, last exit 1), 8 on demand. The two Actions runners and `ai.hermes.gateway` run all
  the time but are on demand: they have `RunAtLoad` and no `KeepAlive`.
- `cargo test --test budget`: an idle serve uses 0.074 % of a core, 46.8 wake-ups a minute and 2.4 MB.
