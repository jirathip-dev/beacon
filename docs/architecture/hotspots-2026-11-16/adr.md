## Summary

Decision: Four of the ten files are lists that every feature adds a line to (modules, commands, what serve starts, the README): each becomes a folder with one file per entry, and a small script writes the lists of modules. CI, the docs and the line cap are split per area; the three files that are not lists stay whole under a size limit.

Slices: (0) this issue's PR does everything except the two Rust moves, and 8 files leave the list; (1) one file per command; (2) serve becomes a folder: a hub and one file per part.

Open questions: (1) Run drover's arch check in beacon's CI too, or only at the end of each lane's turn, where it runs anyway (recommended)? (2) For serve, a hub that parts report to (recommended), or a plain cut into files that is less work but keeps one start-up list every feature edits?

# ADR: hotspots week of 2026-11-16: lists become folders, serve gets a hub (#59)

Status: proposed (architect lane, 2026-11-21). Read `README.md` first: it has the table of all ten files.

## Question

The report lists ten files, and drover runs the PRs that touch a listed file one at a time. Which of them are
lists, which are something else, and what takes each off the list without the same collisions coming back under a
new name?

## Where it stands

- The window is the repo's whole life: first commit 2026-11-12. Every count includes the file's own creation.
- `cli/src/lib.rs` is 17 `pub mod` lines. Every PR that touched it only added lines.
- `cli/src/main.rs` holds the usage text of all five commands, the `match` over them and `target()`, a flag
  parser that returns a five-part tuple to `run`, `serve` and `install` alike. A flag for one command edits all
  three places. `jobs/cli.rs` parses `--config` and `--host` a second time.
- `cli/src/serve.rs` is 492 lines, about 290 of them the one function `serve()`. A feature adds a thread, a field to
  `Live` or a local mutex, and an arm to the route `match`. The same three calls (the notifier's tracker, then the
  history, then the status body) are written out for checks and rules, again for monitors, again for jobs.
- `README.md` is 70 lines and 31 of them are one bullet, `beacon serve`, which five PRs extended.
- `ci.yml` holds five jobs for three areas (the CLI, the app, the repo's own rules) under one concurrency group.
- The cap file is one number. A raise edits it and adds a file to `docs/cap-history/`.
- `docs/architecture/beacon/adr.md` holds D1 to D11. The two newest features already wrote their decisions in
  their own folders (`jobs/`, `history/`) to share no file; the older ones appended here.
- `Cargo.toml`, `install.rs` and `status.rs` are not lists. None has an update round, and two have no predicted
  conflict. They stand at five, the bar, because the repo is nine days old.
- There is no `.drover/arch.toml` yet, and CI does not run drover.

## Options

| | Option | Verdict |
|---|---|---|
| A | Leave it: the counts decay in 14 days | no: every CLI feature so far is a serve feature, so `serve.rs`, `main.rs`, `lib.rs` and the README stay hot as long as work goes on |
| B | Mark all ten `shared` with a size limit, move nothing | no: PRs stop queueing but still collide, each adding at the same place in `serve()` and in the serve paragraph |
| C | Lists become folders, with the module lists written by a script; CI, docs and the cap split per area; `shared` with a tight limit for the three that are not lists | **chosen** |
| D | Collect modules and commands in Rust: a `build.rs` that includes generated `mod` lines, or a registry crate (`inventory`, `linkme`) | no: a build script's `mod` lines need `#[path]` and confuse editors, and a registry is a new dependency with link-time tricks; a 30-line shell script writes plain Rust that is committed and reviewed |
| E | Serve as a plain cut into files, the start-up list written by hand | open question 2: half the work of the hub, but every feature still edits `serve/mod.rs`, its state and its routes |

## Decision

1. **The rule.** A list that features append to is written from a folder by `sh scripts/gen.sh` and never edited.
   Four lists: `cli/src/lib.rs`, `cli/src/cmd/mod.rs`, `cli/src/serve/once/mod.rs`, `cli/src/serve/each/mod.rs`.
   A conflict in one is settled by running the script again. `shapes.md` says what it writes.
2. **Commands.** Each command is `cli/src/cmd/<name>.rs` with `USAGE` and `run`. `main.rs` shrinks to the lookup
   in the generated table and is frozen. `Target` (`--config`, `--host`) replaces the tuple and the second parser
   in `jobs/cli.rs`. `ServeArgs` owns serve's flags and gives `install` the argv to write, so a new serve flag no
   longer touches `main.rs` or `install.rs`.
3. **Serve.** `serve.rs` becomes `cli/src/serve/`: `hub.rs` holds all state two threads share and is the only file
   that locks it; `http.rs` answers; each thread serve runs is a part, one file in `once/` (lives as long as
   serve) or `each/` (started again for each valid config). A part calls the hub and nothing else in serve.
   `report` is the one tap for results; bodies are published by path, so a new endpoint adds no route arm.
   `unix_now`, `every` and `over_budget` move out to `clock.rs` and `budget.rs`, since six lib files import serve
   only for them.
4. **README.** It becomes an index: what beacon is, the install line, one row per command with its docs file.
   Its text moves word for word to `docs/`, one file per area. The README is frozen.
5. **CI.** One workflow per area: `rust.yml`, `swift.yml`, `cap.yml`, `actionlint.yml`. `ci.yml` is deleted and
   frozen, so it cannot come back as the shared file. Job names do not change.
6. **The cap.** The first cap file is frozen. A raise is a new file in `ci/cap.d/` with the number and the reason;
   `scripts/line-cap.sh` enforces the highest.
7. **The M0 ADR.** `docs/architecture/beacon/adr.md` is frozen as it is. A new decision, or an amendment that
   needs new lines, is its own folder under `docs/architecture/`, as `jobs/` and `history/` already are. Nothing
   moves.
8. **Not lists.** `cli/Cargo.toml`, `cli/src/install.rs` and `cli/src/status.rs` are marked `shared` under a limit
   just above their size today (70, 110, 150 lines). When one is full, the next feature takes something out first:
   from `status.rs`, `Rendered` and `rfc3339` are the pieces that are not about status.
9. **Enforcement.** `drover arch-check` reads `.drover/arch.toml`, and `drover turn done` runs it in every lane
   before review. CI's rust job gains one step that reruns the script and fails on a difference. Three layer
   rules and two bans hold the boundaries of `boundaries.md`.
10. **Not done.** No move of `config.rs`, `notify.rs` or the schemas: none is listed. No split of D1 to D11 into
    files: freezing stops the growth. No size rule for all of `cli/src`: only serve's folder gets one, where the
    290-line function was.

## Consequences

- Eight files leave the list when slice 0 merges, `main.rs` with slice 1, `serve.rs` with slice 2 (a file that no
  longer exists is never listed).
- A new module, command or serve part is new files and one run of the script. A stale list fails `turn done` and
  CI with the command to run.
- New files start at zero in next week's report. One that gets hot again was cut along the wrong line.
- The usage text changes (slice 1): a usage error prints the usage line of its own command, as `beacon jobs`
  does today, and an unknown command prints all of them in name order. No test reads more than `usage: beacon`.
- The plist and the unit that `beacon install` writes keep their bytes, so no host is restarted by the change.
- An edit of the number in the cap file still passes arch-check (it judges added lines only). The lane hook
  already refuses that edit; the freeze is what takes the file off the list.
- `swift.yml`'s path check no longer matches every workflow, so a change to `rust.yml` does not start the macOS job.
- Slices 1 and 2 rename what every CLI feature edits; an open CLI PR would take one update round. None is open.
- `git blame` on a moved function shows the slice's commit unless run with `-C`.
- `cli/src` grows by about 220 lines over slices 1 and 2. Headroom is 1,090; no raise is asked.
- If #58 lands a `.drover/arch.toml` first, the lines below are added to it.

Run here, on a throwaway copy of 428da16 with drover's own `arch-check`:

- The slice 0 script rewrote `cli/src/lib.rs` with no difference, and after a new `cli/src/zzz.rs` it added
  `pub mod zzz;` in order.
- With the slice 0 lines below: `arch-check: 234 files, 0 violations`.
- Each rule failed when broken: a line added to `README.md`, to the cap file or to the M0 ADR; a `pub mod` line
  added by hand; a new module with the list not rewritten; `status.rs` at 151 lines.
- A new ADR folder, a new file in `ci/cap.d/` and a new workflow file passed.
- A `[[collect]]` over `.github/workflows/*.yml` switched the freeze of `ci.yml` off, because a collected file is
  never judged. So no such entry is written, and `docs/architecture` is collected by its `README.md` files only.
- All three slices' lines together parse. Tried on today's tree, the layer rules reported the six lib files that
  import serve (13 lines) and nothing else, and both bans caught a part that named another part.
- A check that finds the lists fresh leaves their files untouched, so it causes no rebuild.

Not run here: slices 1 and 2 were not prototyped, so the hub's signatures in `shapes.md` are a sketch that impl
may adjust; what it may not change is `boundaries.md`.

## Open questions for the owner

1. **arch-check in CI.** `drover turn done` already runs it in every lane, and work lands here through lanes.
   Recommended: leave drover out of beacon's CI for now; the one generated-list step needs only `sh` and `git`.
   The other way is a pinned drover download in `rust.yml`, which also judges pushes made by hand.
2. **Serve: hub or plain cut.** The hub (recommended) makes a serve feature new files only and removes the three
   hand-wired copies of the result tap; it is slice 2 as written, about +150 lines. The plain cut moves the same
   code into the same files but leaves `serve/mod.rs` as a hand-written list of state and threads, about +60
   lines; it would be marked `shared` at 120 lines and would collide again at its start-up list.

## The `.drover/arch.toml` lines

Slice 0, this issue's PR. The file is new:

```toml
frozen = [
    "README.md",
    ".github/workflows/ci.yml",
    "docs/architecture/beacon/adr.md",
]
generated = ["cli/src/lib.rs"]
generate = "sh scripts/gen.sh"
shared = ["cli/Cargo.toml", "cli/src/install.rs", "cli/src/status.rs"]

[[collect]]
dir = "docs"
glob = "*.md"

[[collect]]
dir = "ci/cap.d"
glob = "*.toml"

[[collect]]
dir = "docs/architecture"
glob = "*/README.md"

[[size]]
paths = ["cli/Cargo.toml"]
max_file_lines = 70

[[size]]
paths = ["cli/src/install.rs"]
max_file_lines = 110

[[size]]
paths = ["cli/src/status.rs"]
max_file_lines = 150
```

Slice 1, commands. The old list is `cli/src/main.rs`; the collected folder is `cli/src/cmd`:

```toml
frozen = ["cli/src/main.rs"]           # appended
generated = ["cli/src/cmd/mod.rs"]     # appended

[[collect]]
dir = "cli/src/cmd"
glob = "*.rs"

[[layer]]
name = "lib"
paths = ["cli/src/*.rs", "cli/src/jobs/**"]
exclude = ["cli/src/lib.rs", "cli/src/main.rs"]

[[layer]]
name = "cmd"
paths = ["cli/src/cmd/**", "cli/src/main.rs"]
mods = ["crate::cmd", "beacon::cmd"]
may_use = ["lib"]
```

Slice 2, serve. `cli/src/serve.rs` is deleted; the two lists are generated. `cmd`'s `may_use` becomes
`["lib", "serve"]`:

```toml
generated = ["cli/src/serve/once/mod.rs", "cli/src/serve/each/mod.rs"]   # appended

[[layer]]
name = "serve"
paths = ["cli/src/serve/**"]
mods = ["crate::serve", "beacon::serve"]
may_use = ["lib"]

[[ban]]
pattern = "once::"
paths = ["cli/src/serve/once/*.rs", "cli/src/serve/each/*.rs"]
reason = "a part reports to the hub and never names another part"

[[ban]]
pattern = "each::"
paths = ["cli/src/serve/once/*.rs", "cli/src/serve/each/*.rs"]
reason = "a part reports to the hub and never names another part"

[[size]]
paths = ["cli/src/serve/**"]
max_file_lines = 160
```
