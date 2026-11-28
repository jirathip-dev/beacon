## Summary

Decision: Four of the ten files are lists that every feature adds a line to (modules, commands, what serve starts, the README): each becomes a folder with one file per entry, and a small script writes the lists of modules. CI, the docs and the line cap are split per area; the three files that are not lists stay whole under a size limit.

Slices: (0) this issue's PR does everything except the two Rust moves, and 8 files leave the list; (1) one file per command; (2) serve becomes a folder: a hub and one file per part.

Open questions: (1) Run drover's arch check in beacon's CI too, or only at the end of each lane's turn, where it runs anyway (recommended)? (2) For serve, a hub that parts report to (recommended), or a plain cut into files that is less work but keeps one start-up list every feature edits?

# hotspots: beacon week of 2026-11-16 (#59)

Read this file, then `adr.md` (options, decision, consequences, the `.drover/arch.toml` lines), `shapes.md` (the
layout, the types, what the script writes, where each README paragraph and CI job goes) and `boundaries.md` (who
may import whom, and serve's statechart).

## Where it stands (staging 428da16)

The repo's first commit is from 2026-11-12, so the 14-day window is its whole life and every count includes the
PR that created the file. No PR is open; the queue holds #58 (lessons) and this issue.

| file | PRs | predicted conflicts | what the PRs did | ending |
|---|---|---|---|---|
| `cli/src/main.rs` | 9 | 0 | four added a command; the others changed a flag, the usage text or the tuple all commands share | one file per command; frozen |
| `README.md` | 8 | 2 | each CLI feature added to the CLI section, five of them to the one serve paragraph | an index, frozen; the text in `docs/`, one file per area |
| `cli/src/lib.rs` | 8 | 3 | every PR only added `pub mod` lines | written by a script from the folder |
| `cli/src/serve.rs` | 7 | 11 | each feature added a thread, its state and its route inside one 290-line function | a folder: a hub and one file per part |
| `.github/workflows/ci.yml` | 5 | 2 | a job or a step for one area: TestFlight, the app icon, caches, lints | one workflow per area |
| the cap file | 5 | 0 | created, then four raises of its one number | frozen; a raise is a file in `ci/cap.d/` |
| `cli/Cargo.toml` | 5 | 2 | dependencies for serve, notify and TLS, the lints table, the budget feature | shared, 70 lines |
| `cli/src/install.rs` | 5 | 0 | created, ported to Linux, lints, the log folder, `--notify` passed on | shared, 110 lines |
| `cli/src/status.rs` | 5 | 0 | created, then `StatusBody`, `Rendered` for `/config`, lints, the job rows | shared, 150 lines |
| `docs/architecture/beacon/adr.md` | 5 | 0 | a decision (D10, D11) or an amendment per feature | frozen; a decision is a new folder |

`serve.rs` is the real one: 11 predicted conflicts and 4 update rounds, because every CLI feature so far is a serve
feature and they all meet in `serve()`.

## How the callers read, after

A new serve feature (say `/uptime`) is new files only:

```text
cli/src/uptime.rs                  its logic: pure, testable without serve
cli/src/serve/once/uptime.rs       pub fn start(s, hub): its thread; hub.publish("/uptime", body)
docs/uptime.md                     its docs
docs/architecture/uptime/          its ADR
cli/tests/uptime.rs                its test
sh scripts/gen.sh                  rewrites cli/src/lib.rs and cli/src/serve/once/mod.rs from the folders
```

A part talks to the hub and to nothing else in serve:

```rust
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub<'s>, config: &Arc<Loaded>) {
    for c in config.host.checks.clone() {
        let generation = config.generation;
        s.spawn(move || {
            loop {
                let (status, message) = probe::run(&c.command, Duration::from_secs(c.timeout_s.into()));
                if !hub.report(generation, Key::Check(&c.id), status, message) {
                    return;
                }
                thread::sleep(every(c.every_s.into()));
            }
        });
    }
}
```

That is `cli/src/serve/each/checks.rs`, less the budget tests' switch. `report` feeds the notifier, the history and `/status` in one call;
today each feature wires those three by hand. It returns false once a newer config is live, and the thread ends.

A new command is one file, `cli/src/cmd/<name>.rs`, with its usage line and its entry point:

```rust
pub const USAGE: &str = "beacon run [--config <path>] [--host <name>]";

/// `beacon run`: this host's checks once; exit 0 all up, 1 a check down.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    let (target, rest) = Target::take(args)?;
    if !rest.is_empty() {
        return Err(format!("usage: {USAGE}"));
    }
    let up = crate::run::round(&target.host()?);
    Ok(if up { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
```

A new serve flag is an edit to `cli/src/cmd/serve.rs` alone: `ServeArgs` parses it and hands `install` the argv to
write, so `install.rs` no longer names serve's flags.

A list that is out of date fails the lane's `drover turn done` and CI's rust job, and says what to run:

```text
cli/src/lib.rs: differs from what `sh scripts/gen.sh` writes: run it and commit the output, never hand-merge a generated file
```

A cap raise is one new file, `ci/cap.d/2026-11-24-8000.toml`, holding the number and the reason. A new decision is a
new folder under `docs/architecture/`. Docs for a feature go in the `docs/` file of its area, or a new one.

## Slices

**Slice 0, this issue's PR.** Moves no Rust code; `cli/tests/` is not touched.

- `scripts/gen.sh`, which writes `cli/src/lib.rs`: the same bytes as today.
- `README.md` becomes an index; its text moves word for word to `docs/run.md`, `serve.md`, `alerts.md`,
  `rules.md`, `install.md` and the existing `jobs.md`.
- `ci.yml` becomes `rust.yml`, `swift.yml`, `cap.yml` and `actionlint.yml`. Job names stay as they are.
- `scripts/line-cap.sh` takes the highest cap of the first cap file and `ci/cap.d/*.toml`, with one more test case.
- `.drover/arch.toml` is created with the slice 0 lines of `adr.md`.

Eight files leave the list when it merges. Then two issues, which the orchestrator files on approve:

| | title | moves | proof it changed nothing | queue as |
|---|---|---|---|---|
| 1 | `cli: one file per command` | `main.rs` and `jobs/cli.rs` to `cli/src/cmd/`; `Target`, `ServeArgs`; `install` takes an argv | `cli/tests/` passes untouched; the plist and unit keep their bytes | plain: the usage text changes (`adr.md`) |
| 2 | `cli: serve as a folder` | `serve.rs` to `cli/src/serve/`; `unix_now`, `every` to `clock.rs`, `over_budget` to `budget.rs` | `cli/tests/` passes untouched, the budget tests included | `--refactor`, after 1 |

Both rename the files every CLI feature edits, so they should run while no CLI feature PR is open, as now.

Lines: slice 0 adds none to `cli/src`; slice 1 about +70 and slice 2 about +150, all file headers, signatures and
the hub's type. Headroom is 1,090, so no raise is asked.
