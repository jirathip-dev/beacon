# Statecharts

## `beacon serve`: process (ADR D3, D4)

```
 start ─▶ load config ──invalid / no hosts.<this> / bind fails──▶ exit 2
              │ ok
              ▼
          Serving: listen; body = {checks: {}}; spawn one Check thread per hosts.<this>.checks[]
              │
              ├─ GET /status, If-None-Match == etag ─▶ 304          (never waits on a check)
              ├─ GET /status ───────────────────────▶ 200 body, ETag, Cache-Control: no-cache
              ├─ any other path ────────────────────▶ 404
              ├─ every 300 s: reload ─ unchanged (304 / same mtime) ─▶ Serving
              │                       ─ invalid ─▶ log, keep old config ─▶ Serving
              │                       ─ valid ─▶ stop threads of removed/changed checks, drop their results,
              │                                  start threads of new/changed checks ─▶ Serving
              └─ a Check thread panics ─▶ abort (panic = "abort") ─▶ supervisor restarts serve
```

## `beacon serve`: one check thread

```
      ┌──────────────────────────────────────────────────────────────┐
      ▼                                                              │
   Running ── exit 0 ──────────▶ (up,   last line)  ─┐               │
   sh -c cmd ─ exit ≠ 0 ───────▶ (down, last line)  ─┤               │
   (own pgid) ─ past timeout_s ─▶ kill -pgid,         │               │
                                  (down, "timeout after <n>s") ─┤      │
                                                      ▼               │
                                   record(): changed? ── yes ─▶ re-render bytes + ETag
                                                      │ no            │
                                                      ▼               │
                                              Sleeping every_s ───────┘   (stop signal from reload: exit)
```

`every_s` is measured from the end of one run to the start of the next, so a slow check never overlaps itself.

## App: refresh (ADR D2, D3, D8)

```
 idle ─(appear, pull-to-refresh, 60 s foreground timer, BGAppRefreshTask)─▶
   path unsatisfied? ── yes ─▶ snapshot: every row unknown "offline" ─▶ idle
        │ no
        ▼
   load config: .local → file
                .url  → GET If-None-Match; 200 → cache + use; 304 or error → cached copy (configAt shown)
                        no cached copy and error → snapshot: one row down "config: <error>" ─▶ idle
        ▼
   probing: one task group running every http/tcp probe + one GET per host (If-None-Match), each with its timeout
        ▼
   map heartbeat monitors from their host's body (below) ─▶ snapshot saved ─▶ WidgetCenter.reloadAllTimelines() ─▶ idle
```

## App: a heartbeat monitor's row

```
   host fetch failed / timeout / non-2xx / undecodable ─▶ down  "host unreachable: <reason>"
   200, or 304 with a cached body:
       checks[id] absent ──────────────────────────────▶ down  "no result yet"
       checks[id].status == down ──────────────────────▶ down  message · since
       else ───────────────────────────────────────────▶ up    message · since
```
