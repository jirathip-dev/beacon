# Statecharts

## CLI: one beat per `beacon run` (ADR D4)

Per heartbeat id on this host. `rec` = the id's `Written` record in local state.

```
                 ┌──────────────────────────────────────────────────┐
  run check      │                                                  │
  → (status,msg) ▼                                                  │
           ┌───────────┐  no rec, or status != rec.status,          │
           │  Decide   │  or now - rec.written_at >= 30 min          │
           └───────────┘ ─────────────────────────────────────────▶ Write
                 │ else                                              │ PUT beats/<id>.json (sha = rec.sha)
                 ▼                                                   │
              Skip                                       ┌───────────┴───────────┐
          (print, state unchanged)                    2xx│                       │409 / error
                                                         ▼                       ▼
                                             rec = {status, now, new sha}   409: re-GET sha, retry once
                                             state.save()                   still failing: rec unchanged,
                                                                            print error, exit 2 at the end
```

Exit code is the max over ids: 0 all up · 1 any down · 2 any config/network error. A down
that was written cleanly is exit 1, not 2.

## App: a heartbeat monitor's display state (ADR D5)

```
   beat file missing ──────────────▶ down  "no beat yet"
   beat.status == down ────────────▶ down  beat.message
   now - beat.time > max_age_min ──▶ down  "stale, last beat <time>"
   else ───────────────────────────▶ up    beat.message
   fetch failed (network, 401) ────▶ unknown  (kept out of `worst` = down)
```

## App: refresh

```
  idle ─(foreground, pull-to-refresh, BGAppRefreshTask)─▶ fetching config
       ─▶ fetching beats ─▶ checking (all monitors concurrently, each with its timeout)
       ─▶ snapshot saved ─▶ WidgetCenter.reloadAllTimelines() ─▶ idle
  any step failing ─▶ snapshot saved with rows = unknown + error string ─▶ idle
```
