# Shapes and layout (#59)

## The layout, after all three slices

```text
scripts/gen.sh                     writes the four generated lists below
.drover/arch.toml                  the rules of adr.md's last section
ci/cap.toml                        frozen: the first cap
ci/cap.d/<date>-<cap>.toml         one file per raise
.github/workflows/rust.yml         job rust
.github/workflows/swift.yml        jobs changes, swift
.github/workflows/cap.yml          job line-cap
.github/workflows/actionlint.yml   job actionlint
.github/workflows/apple-testflight.yml   as today
README.md                          frozen: what beacon is, the install line, one row per command with its docs file
docs/run.md serve.md alerts.md rules.md jobs.md install.md

cli/src/
  main.rs            frozen: finds the command by name in cmd::ALL, prints an error, exit 2
  lib.rs             generated
  cmd/mod.rs         generated: the commands and their table
  cmd/check.rs  run.rs  serve.rs  install.rs  jobs.rs       one command each: USAGE and run
  target.rs          Target: --config and --host, shared by every command
  clock.rs           unix_now, every (BEACON_SPEEDUP)             from serve.rs
  budget.rs          over_budget (the budget tests' switch)       from serve.rs
  serve/mod.rs       serve(): read the config, bind, build the hub, start the parts, answer
  serve/hub.rs       Hub, Loaded, Notifier: all state that two threads share
  serve/http.rs      the answer loop: path to body, ETag, 304, 404
  serve/once/mod.rs  generated
  serve/once/        parts that live as long as serve:
    jobs.rs            job discovery every 60 s
    keep.rs            the 1 s loop: history tick and save, tidy, git pull, config re-read
    sample.rs          the sampler, the metric rules, /metrics
    send.rs            with --notify: the delivery thread and the alive thread
    waste.rs           the budget tests' mem, cpu and wake switches
  serve/each/mod.rs  generated
  serve/each/        parts started again for each valid config:
    checks.rs          one thread per check
    hosts.rs           with --notify: one thread per other host
    probes.rs          on the prober host: the monitor probes, /history/<id>
  install.rs  status.rs                        shared, whole
  config.rs  history.rs  jobs.rs  jobs/  metrics.rs  monitor.rs  net.rs  notify.rs  out.rs  probe.rs
  prober.rs  rules.rs  run.rs  sampler.rs  state.rs             as today
```

`jobs/cli.rs` becomes `cmd/jobs.rs`. `cli/src/serve.rs` is deleted. Public paths that tests import
(`beacon::status`, `beacon::notify`, `beacon::jobs`, `beacon::probe` and the rest) do not change.

## What `sh scripts/gen.sh` writes

POSIX `sh`, run from any folder, names sorted under `LC_ALL=C`. It writes a file only when its bytes change, so a
check does not make cargo rebuild. Its output passes `cargo fmt --check`.

| file | from | content |
|---|---|---|
| `cli/src/lib.rs` | every `cli/src/<name>.rs` but `lib.rs` and `main.rs`, and every `cli/src/<name>/mod.rs` | `pub mod <name>;` a line |
| `cli/src/cmd/mod.rs` | every other `cli/src/cmd/<name>.rs` | below |
| `cli/src/serve/once/mod.rs` | every other `.rs` there | `mod <name>;` a line, then `start(s, hub)` calling each `<name>::start(s, hub)` |
| `cli/src/serve/each/mod.rs` | every other `.rs` there | the same, with `start(s, hub, config)` |

```rust
pub mod check;
pub mod install;
pub mod jobs;
pub mod run;
pub mod serve;

use std::process::ExitCode;

/// One `beacon <name>` command: its usage line and its entry point.
pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub run: fn(&[&str]) -> Result<ExitCode, String>,
}

#[rustfmt::skip]
pub const ALL: &[Command] = &[
    Command { name: "check", usage: check::USAGE, run: check::run },
    Command { name: "install", usage: install::USAGE, run: install::run },
    Command { name: "jobs", usage: jobs::USAGE, run: jobs::run },
    Command { name: "run", usage: run::USAGE, run: run::run },
    Command { name: "serve", usage: serve::USAGE, run: serve::run },
];

/// Every command's usage line, for an unknown or missing command.
pub fn usage() -> String {
    let lines: Vec<_> = ALL.iter().map(|c| c.usage).collect();
    format!("usage: {}", lines.join("\n       "))
}
```

Slice 0 writes the first row only; slices 1 and 2 add theirs. A command's name is its file's name.

## Commands (slice 1)

```rust
/// The config file and the host in it that a command acts on.
pub struct Target {
    pub path: String,
    pub name: String,
}

impl Target {
    /// Takes `--config <path>` (else `$BEACON_CONFIG`) and `--host <name>` (else this machine's) out of `args`.
    pub fn take<'a>(args: &[&'a str]) -> Result<(Target, Vec<&'a str>), String>;
    /// Reads and validates the file and returns this host's block.
    pub fn host(&self) -> Result<Host, String>;
}

/// What `beacon serve` is started with; `beacon install` writes the same.
pub struct ServeArgs {
    pub target: Target,
    pub listen: String,
    pub notify: Option<String>,
}

impl ServeArgs {
    pub fn parse(args: &[&str]) -> Result<ServeArgs, String>;
    /// `serve --config <abs> --host <name> --listen <addr> [--notify <abs>]`: what a plist or unit runs.
    pub fn argv(&self) -> Result<Vec<String>, String>;
}

pub fn install(serve: &[String]) -> Result<String, String>;
```

`Target` replaces the five-part tuple in `main.rs` and the second flag loop in `jobs/cli.rs`. `install` writes the
argv it is given: one `<string>` per argument in the plist; in the unit, an argument with a character outside
`[A-Za-z0-9._:-]` is quoted. That reproduces today's files byte for byte, so `beacon install` on a host that is
set up still answers `= already`.

## Serve (slice 2)

```rust
/// One valid config as serve uses it; `generation` counts the valid configs since start, from 1.
pub struct Loaded {
    pub generation: u64,
    pub raw: Vec<u8>,
    pub host: Host,
    pub others: BTreeMap<Id, Host>,
    pub monitors: Vec<Probed>,
    pub rules: Vec<RuleCheck>,
    pub policy: NotifyPolicy,
}

/// What `--notify` adds: where events go and what the host watchers share.
pub struct Notifier<'a> {
    pub file: &'a Notify,
    pub down: Down,
    pub watched: Watched,
    events: Sender<Event>,
    inbox: Mutex<Receiver<Event>>,
}

pub struct Hub<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub state: Option<PathBuf>,
    pub notifier: Option<Notifier<'a>>,
    live: Mutex<Live>,
    board: Mutex<BTreeMap<String, Rendered>>,
}

struct Live {
    config: Arc<Loaded>,
    body: StatusBody,
    own: Tracker,
    history: History,
}

impl<'a> Hub<'a> {
    /// Reads and validates the config: generation 1. The error is serve's exit message.
    pub fn open(target: &'a Target, notify: Option<&'a Notify>) -> Result<Hub<'a>, String>;
    /// The config that is live now.
    pub fn config(&self) -> Arc<Loaded>;
    /// A new valid config: the next generation. Publishes `/config`, drops the results and `/history/<id>`
    /// bodies of what it no longer names, starts `each::start`; returns the number of checks.
    pub fn load<'s>(&'s self, s: &'s Scope<'s, '_>, raw: Vec<u8>) -> Result<usize, String>;
    /// One result of a check, rule or monitor: to the notifier, the history and `/status`.
    /// False when `generation` is over: the caller's thread ends.
    pub fn report(&self, generation: u64, key: Key, status: Status, message: String) -> bool;
    /// This minute's job rows; they replace the last ones.
    pub fn report_jobs(&self, rows: &[Row]);
    /// Queues one event for delivery; nothing without `--notify`.
    pub fn send(&self, event: Event);
    /// The history's minute tick; returns the `/history` body to save.
    pub fn tick(&self) -> Rendered;
    /// Makes `body` the answer to `GET <path>`.
    pub fn publish(&self, path: &str, body: Rendered);
    /// The answer to `GET <path>`; `None` is a 404.
    pub fn body(&self, path: &str) -> Option<Rendered>;
}
```

What goes away: `Cell`, `Cells` and the five route arms (the board is keyed by path); `Live.rules`, `Live.notify`
and the `reviews` mutex (each is read from `hub.config()`); the `probed` closure and the three hand-wired copies of
"tracker, then history, then body" (`report` is the one). A request still only clones an `Arc` under a lock held
for a map lookup, so D4's "nothing waits" holds; the budget tests' `lock` switch holds the board's lock.

Moved as it is, with no change in behaviour: the sampler reads `disks` once at start; a watcher of a host that
survives a reload keeps its thread and its baseline (`Notifier.watched`); nothing is sent at start or on a reload.

## A cap raise (slice 0)

```toml
line_cap = 8000
why = "#N: owner decision qNNNN, what it covers"
```

`ci/cap.d/<date>-<cap>.toml`. `scripts/line-cap.sh` enforces the highest `line_cap` among `ci/cap.toml` and
`ci/cap.d/*.toml`, which is the rule `drover cap` already reads. The `why` replaces a new file in
`docs/cap-history/`; the five files there stay.

## Where the README's text goes (slice 0), word for word

| README lines today | to |
|---|---|
| 1 to 6: title, what it is, the install line | stay |
| 8 to 18: `beacon check`, `beacon run` | `docs/run.md` |
| 20 to 26: `beacon serve`, its endpoints, the reload, the pull | `docs/serve.md` |
| 27 to 34 and 39 to 47: `--notify`, batches, reminders, the notify file, the signature, `alive` | `docs/alerts.md` |
| 35 to 39: metric rules | `docs/rules.md` |
| 48 to 53: job discovery, `beacon jobs` | `docs/jobs.md`, where it is not already said |
| 55 to 70: `beacon install`, the Mini's setup, the boat VMs | `docs/install.md` |

The README keeps one table row per command: the command, eight words on what it does, its docs file.

## Where CI's jobs go (slice 0)

| workflow | jobs | note |
|---|---|---|
| `rust.yml` | `rust` | gains one step: `sh scripts/gen.sh && git diff --exit-code` |
| `swift.yml` | `changes`, `swift` | `changes` matches `app/`, `.swiftlint.yml` and `swift.yml` itself, no longer every workflow |
| `cap.yml` | `line-cap` | |
| `actionlint.yml` | `actionlint` | |

Each keeps `on: pull_request` and `push` to `main` and `staging`, and its own concurrency group
(`<workflow>-${{ github.ref }}`): with one shared group the four would cancel each other. Job names are the check
names GitHub and drover see, and they do not change.
