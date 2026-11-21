# Boundaries and state (#59)

## Who may import whom

An arrow means "may import". `drover arch-check` enforces every line marked with its rule.

```text
main.rs ──▶ cmd ──▶ serve ──▶ lib                                     [[layer]] cmd, serve, lib
             └────────────────▶ lib

lib    config, status, probe, net, notify, history, jobs, metrics, monitor, prober, rules, run, sampler,
       state, install, target, clock, budget, out. Never imports serve or cmd.
cmd    one file per command. Imports lib and `serve::serve`. `cmd/install.rs` imports `cmd/serve.rs` for
       `ServeArgs`; no other command names another.
serve  mod.rs ──▶ hub, http, once, each
       hub    ──▶ each (a new config starts its parts), lib
       http   ──▶ hub
       once/<part>, each/<part> ──▶ hub, lib; never another part, never http        [[ban]] once::, each::
```

Today six lib files import serve for `every`, `unix_now` or `over_budget` (`history`, `jobs`, `monitor`, `notify`,
`prober`, `state`): the `lib` layer rule reports each of them on the current tree. Slice 2 moves the three
functions to `clock.rs` and `budget.rs`, and the rule then keeps lib from reaching up again.

Only `hub.rs` locks `live` and `board`. A part never sees a `Mutex` of serve's: it calls `config`, `report`,
`report_jobs`, `send`, `tick`, `publish`. State that one part alone needs (the sampler's ring, the rule streaks,
what `keep` last saw, pulled and saved, the jobs watcher) stays a local of that part's thread, as today.

## `beacon serve`: configs and generations

```text
 start ─▶ Hub::open: read the config ── unreadable / invalid / no hosts.<this> ─▶ exit 2
              │ ok: generation 1
              ▼
          bind ── fails ─▶ exit 2
              │ ok
              ▼
      ┌─▶ Serving(n)
      │     each::start(n): checks, hosts, probes run; every report carries n
      │     once::start, at n = 1 only: jobs, keep, sample, send, waste run until serve exits
      │     GET <path> ─▶ board[path]: 200 with ETag, or 304 on a matching If-None-Match; no entry ─▶ 404
      │
      │     keep, every second: the file's bytes changed?
      │        ├─ no ───────────────────────────────▶ Serving(n)
      │        ├─ yes, invalid ─▶ log once ──────────▶ Serving(n): /config and the checks stay the last valid
      │        └─ yes, valid ─▶ hub.load:
      │               generation = n + 1; publish /config
      │               drop from /status, the history and the board what the config no longer names
      └───────────────each::start(n + 1)
```

## One thread of an `each` part

```text
   running(n) ─▶ probe ─▶ hub.report(n, …)
                             ├─ true:  generation is still n ─▶ sleep ─▶ running(n)
                             └─ false: a newer config is live ─▶ the thread ends; nothing was recorded
```

A host watcher (`each/hosts.rs`) is the one exception, as today: a host named by both the old and the new config
keeps its thread, so its debounce baseline survives a reload and no alert is lost or repeated. Its thread ends
when a config no longer names the host.

A `once` part reads `hub.config()` at the top of each cycle and reports with that config's generation, so a rule
or a job judged against an old config is never recorded under the new one.
