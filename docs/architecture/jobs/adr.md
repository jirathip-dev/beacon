## Summary

Decision: `beacon serve` lists its user's launchd or systemd jobs once a minute and puts each in `/status` as a row that already carries its name and group. A row with no group is unreviewed, and the app shows it under Unreviewed; the app never reads the new `jobs` block. Up or down follows the job's own kind. `beacon jobs keep|ignore|removed` rewrites only one host's `jobs` block and prints the commit to run.

Slices: (1) the `jobs` block in the config and schema; (2) discovery on macOS, the down rule and the `/status` rows; (3) the `new job found` webhook and the budget test with 50 fake jobs; (4) `beacon jobs`; (5) discovery on Linux; (6) the app's rows and Unreviewed group; (7) the Mini's migration and the README.

Open questions: (1) Should an unreviewed job that vanishes just leave the list? I made only kept jobs go down on vanishing, because drover's one-shot wake jobs would each page you and stay red. (2) The first start finds 15 unreviewed jobs on the Mini. I send no webhook for that first list (they show in the app). Send all 15 instead? (3) The boat's runners are system units, which `systemd --user` cannot see, so `runner` stays a shell check. Or should Linux also read system units? (4) The Mini's two Actions runners are not KeepAlive, so they go down only on a failed exit, not whenever they are not running as today. Accept, or add an optional "must be running" mark on a kept job?

# ADR D12: serve discovers launchd and systemd user jobs (#39)

Status: proposed. It continues D1–D11 in `../beacon/adr.md` and lives in its own folder so that file is not
edited again. Read `design.md` first (usage, shapes, statecharts, tests), then this.

## Question

Every job Beacon watches is a hand-written shell line plus a monitor entry, and a job nobody added is not
watched at all. The owner decided (t151: q2447, q2449, q2450, q2451, q2452) that serve discovers the jobs,
judges each by its kind, shows new ones as Unreviewed with one webhook, and that a `beacon jobs` command edits
a `jobs` block the owner commits. Those are not reopened. What is left: where the review lives, how the app
learns of a job it has no config for, what serve reads and how often, what "missed 2 intervals" means, what is
remembered on disk, and how a command edits one block without touching the rest of the file.

## Decisions

### 1. The review is `hosts.<name>.jobs`: one entry per label, one of three values

A map from label to `"ignore"`, `"removed"` or `{"keep": {"name": …, "group": …}}`. A label not in the map is
unreviewed. Options turned down:
- Three lists (`keep`, `ignore`, `removed`). A label could sit in two of them; the map cannot hold that state.
- A top-level `jobs.<host>`. `checks` and `disks` are already per host, and serve reads only its own host.
- `keep` writes a heartbeat monitor. That is two places per job, and the tool would edit the list whose order
  the owner sets by hand.

### 2. `/status` rows describe themselves; the app does not read `jobs`

A job's row in `/status` has the id `job-<slug>` and two new optional fields: `name` (the kept name, else the
label) and `group` (kept jobs only). The app shows every row that has a `name`, under its `group`, and under
Unreviewed when it has none. So the review state is decided in one place (serve, which reads the config
strictly) and the app needs no slug rule and no knowledge of labels. Options turned down:
- The app joins the config's `jobs` with `/status`. It would need the slug rule, and an unreviewed job is by
  definition not in the config.
- Serve sends `"group": "Unreviewed"`. A kept group of that name could not be told apart; the missing field
  is the type.

An older app ignores both fields (D1) and shows no job rows. An older serve sends none.

### 3. Status comes from the listing; only the kind and the run count need a read per job

One process a cycle (60 s) lists every job with whether it runs and how it last exited. That is enough for
up or down once the kind is known. A second process reads one job (`launchctl print`, `systemctl show`) only
when the job is new, when its line in the listing changed, or when a scheduled job's interval has passed.
The last trigger is the one addition to the issue's "only for jobs whose state changed": a run shorter than
a cycle never shows in the listing, so "missed" needs the run count (§4).
- `launchctl list`'s format is the one launchd promises to keep (its man page); `print`'s is "not API". So
  the part that can break on an OS update is the kind and the run count, and a job whose read does not parse
  is judged as an on-demand job from the stable listing. It never disappears and serve never stops.
- Turned down: reading every job every cycle (24 processes a minute on the Mini for nothing), and the
  domain dump `launchctl print gui/<uid>` as the listing (114 KB of "not API" text a cycle).
- Beacon's own job (`dev.beacon.serve`, `beacon-serve.service`) is left out with `com.apple.*` and
  `application.*`: when serve is down the host is unreachable, which the app already shows.

### 4. The down rule, by kind

| kind (read from the job) | down when |
|---|---|
| KeepAlive (launchd `keepalive`; systemd `Restart=` not `no`) | it is not running |
| scheduled (launchd `StartInterval`, `StartCalendarInterval`; systemd timer) | its last exit failed, or it missed 2 intervals |
| on demand (anything else: `RunAtLoad` only, `WatchPaths`, a plain service) | it is not running and its last exit failed |
| any kept job | it is not in the listing ("not loaded") |

- A scheduled job is judged by its last exit even while its next run is in progress, so a job that fails
  every run does not flip to up for the length of each run.
- **Missed 2 intervals** means: two reads in a row, one interval apart, found the run count unchanged. The
  count is launchd's `runs`, or the time systemd last started the service. A run too short for the listing
  to catch still moves it, and a hung run does not, so a stuck `tick` goes down after two intervals. A
  calendar job's interval is the coarsest field it sets (hour and minute: a day). The count is kept in memory
  only: after a restart serve needs two intervals again. Turned down: persisting it (a disk write a minute
  for the Mini's 60 s job) and judging by wall time since the last run (launchd shows no such time, and a
  host that slept would read as missed).
- The third kind is mine, not the owner's: the issue names two. Open question 4 is its one visible effect.

### 5. Only a kept job that vanishes is down

The issue says any job. I narrowed it (open question 1): an unreviewed job that vanishes leaves the list.
`dev.drover.t135-wake` was in the issue's list earlier today and is gone now; under "any job" each such job
is a webhook, then a down alert, then a red row until someone runs `beacon jobs removed`. With the narrower
rule serve needs no memory to know what vanished: the config's kept labels are the list. `removed` replaces a
kept entry, and a `removed` job that is back in the listing shows as unreviewed.

### 6. What is remembered: the labels already announced, one file per host, only on the notifying host

`~/.local/state/beacon/jobs/<host>.json` holds every label for which `new job found` was sent, or which was
there at the baseline. A label is written before its webhook is queued, so a crash sends at most one. The file
only grows. A missing file makes the host's first report the baseline: recorded, nothing sent (open question
2), as D11 sends nothing at start. Each file has one writer: the discovery thread for serve's own host, and
the host's existing poll thread for each other host. So there is no lock, and a boat needs no notifier. The
state folder's trim never deletes `jobs/`. Turned down: a "new" flag in `/status` (it needs a time window),
and one shared file (two writers).

### 7. `beacon jobs` reads the host once; its edits replace one byte range

- `beacon jobs` runs one listing and one read per job through the same code as serve, so it prints what serve
  would report. It cannot show "missed", which needs two reads an interval apart.
- `keep`, `ignore` and `removed` are pure file edits: they need no running serve and no live job, and take
  `--host` to edit another host's block. The edit finds the bytes of `hosts.<host>.jobs` and replaces only
  those with the block written in one fixed form (one label a line, sorted). When the host has no block, it
  is inserted after the host's last field. Running a command twice changes nothing the second time. The
  range is found with serde_json's borrowed raw values (a feature flag of a crate already in the tree).
  Turned down: parse and re-serialize the file (every byte would change), and a new JSON-editing crate.
- Each edit prints the `git commit … && git push` line for the config's checkout. Serve re-reads the file
  when it changes on disk, and other hosts get it by their pull.

### 8. Ids: `job-<slug>`, and `job-` is reserved

The slug is the label in lower case, with every character outside `a-z0-9` as `-`, at most 60 characters. When it would
be longer, or two labels of a host share it, the id ends in 8 hex characters of the label's SHA-256. `beacon
check` refuses a `checks[].id` that starts with `job-`, so a job can never hide a check. A heartbeat monitor
cannot name a job (its id must be in `checks`): there is one way to show a job.

### 9. Linux: the user's own units, a timer and its service as one job

The jobs are the `.service` and `.timer` units whose files are in `~/.config/systemd/user`; units the
distribution ships are left out as `com.apple.*` is. A timer and the service it starts are one job under the
timer's name. The boat's `runner` check stays a shell check (open question 3): its command calls `systemctl`
without `--user`, so those runners are system units.

### 10. Tests name the tool, so a developer's own jobs never leak in

Serve calls the program named in `$BEACON_LAUNCHCTL` or `$BEACON_SYSTEMCTL` (the switches `install` already
has). The variable that is set also picks the manager, so CI's Linux runner tests the launchd path and a Mac
the systemd path; with neither set, the system picks. Every test that starts serve sets one to a fake that
answers from fixture files. The verify recipe's live serve does the same, so its screenshot is the same on
any host.

## Consequences

- D3's `/status` gains two optional fields per row, and D4's "serve's only disk writes are the rotation and
  the trim" gains a third: the announced file, on a new label only.
- D11's webhook gains `"kind": "job"`. A job's up and down go out as `"kind": "check"` like any other row,
  for unreviewed jobs too.
- Budget (D4). Measured on the Mini today: `launchctl list` 6 ms, `print` 5 ms; an idle serve uses 0.074 % of
  a core and wakes 46.8 times a minute against limits of 0.2 % and 70. Discovery adds one process a minute,
  one read per scheduled job per interval (about 2 a minute on the Mini) and one read per job at start.
  The budget test runs it with 50 fake jobs, 10 of them scheduled.
- On the Mini, day one: `dev.drover.clean` is down (last exit 1, as its hand check says today), `ttyd` is up
  (it runs; its exit 1 was an earlier run), and `com.openssh.ssh-agent` shows as unreviewed because Apple
  ships it outside `com.apple.*`.
- Kept jobs come after the monitors of their group, sorted by name, because `/status` has no order. Today's
  nine rows sit between `drover doctor` and `CLIs on PATH`; after the migration they follow `Disk /`.
- When a host is unreachable the app keeps its job rows from the last snapshot and shows them down with the
  heartbeat rows, so the count of down rows does not shrink when a host dies.
- Rollout order: the app first, then the CLI on every host, then the `jobs` block and the removal of the old
  checks. A CLI from before this issue rejects `jobs` and keeps serving its last valid config (D1).
- The systemd reads are from systemd's documentation, not from a capture: there is no Linux host in this
  lane. Slice 5 captures the fixtures on a boat first; a difference is a fix inside `jobs/systemd.rs`.
- To check in slice 4: whether `launchctl list` from an SSH shell sees the same session as serve's. If it
  does not, `beacon jobs` says so instead of printing kept jobs as not loaded.
- About 750 non-test lines (Rust about 700, Swift about 50). The tree is at 3604 of 4500.

## Hot files and `.drover/arch.toml`

This issue converts no hot file, so the impl lane adds no `.drover/arch.toml` lines. Five files it could
touch were edited by 5 or more PRs in the last 14 days, and the design keeps out of them:

- `docs/architecture/beacon/adr.md` (6): not edited. D12 is this folder.
- `cli/src/main.rs` (8): one match arm and one usage line. The command parses its own options in
  `cli/src/jobs/cli.rs`.
- `cli/src/serve.rs` (5): one spawned thread and one hand-over on reload. The thread's body is `jobs::watch`.
- `README.md` (6): the two setup paragraphs change and one line links to a new `docs/jobs.md`, which holds
  the command's reference.
- `app/Beacon/Views.swift` (5): not edited. Unreviewed is an ordinary group card; the rows are built in
  `BeaconKit`.
