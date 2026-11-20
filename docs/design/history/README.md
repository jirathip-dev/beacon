## Summary

Decision: Every row with history gets a `›`: an http or tcp monitor opens its own screen (uptime, a 30-day strip, a response-time chart and the incident log), and a check on the host detail opens the same screen without the chart. Uptime is two numbers, last 24h and last 30d, floored so 100% means not one down second, labelled "as seen from macmini" with the time not watched spelled out. The chart opens on 24h and switches to 1h or 30d; probes with no response are red ticks on its baseline. The log shows the last 5 incidents, newest first, an open one on top in red.
Screens: monitor history (Blog, at 24h, 1h while scrubbing, 30d), check history (backup on macmini), and the states page (entry rows, down now, young monitor, loading, no prober, not served, unreachable).
Open questions: none. (Revised per the owner: the chart opens on 24h, the log shows 5 incidents, the cert line stays without "from this phone", and the explaining sentences are gone.)

## Images (`png/`, 390 wide at 3×, each `-dark` and `-light`; the `-1440` sheets show both side by side)

| image | decision it shows |
|---|---|
| `monitor-24h-*.png`, `monitor-1440.png` | **Monitor history** (http/tcp, from `/history` and `/history/<id>` on the `prober` host). Top bar `[‹ monitors]`, the name, `group · type` under it. Then the target and the certificate line (from the phone's own probe, not labelled so). **`~ uptime`**, "as seen from macmini": last 24h and last 30d (the formula in `history.md`), floored to two decimals. A strip of 30 day cells: red a day with down time, green a watched day without, `edge` a day not watched at all, an empty outline before the monitor was first seen. Under it: `watched 29d 19h · 4h 30m not watched on Nov 14` (the gaps, so a sleeping Mini never reads as downtime). **`~ response time`** with `[1h] [24h] [30d]`: 24h plots the mean of 7-min buckets of `recent`; y from 0 to a round top over the plotted line; now, min, avg, max below from the raw probes. A probe with no response (`ms: null`) is a red tick on the baseline, never drawn as 0; the only footnote is `▮ 3 no response`, and none when there are none. **`~ incidents`**: newest first, `down at → up at`, duration on the right, the message under it; `3 of 3 · 30d` in the header. |
| `monitor-1h-*.png` | **1h**: every probe, hold and drag reads `HH:mm  ms` (or `no response`), as the host chart's scrub. |
| `monitor-30d-*.png` | **30d** from `hourly`: the avg line over 4-hour buckets, shaded up to the bucket's max (legend: avg, max). An hour with no response breaks the line and the band. |
| `check-*.png`, `check-1440.png` | **Check history** (a check, and later #41's rules and #39's jobs, from its own host's `/history`): `[‹ macmini]`, the same uptime card and log, no chart and no note about it. An open incident leads the log in red: `down since 03:12 · 6h 19m so far`. |
| `states-*.png`, `states-1440.png` | **Entry**: with `prober` set, http/tcp rows on the list get `›`; the host detail's check rows get `›` too. **Down now**: API down for its certificate, the open incident on top; its cert line in red from the phone. **Young**: smtp first seen 3 days ago: its 30d label reads `3d 4h, all it has` and the strip is empty before that. **Loading**: `↻ reading the log`, alone. **No prober**: `no history`, alone. **Not served** (404): the lighthouse art over `dark lamp · no history`, no sentence. **Unreachable**: an amber `● history from 09:12 · timeout after 5s` over the last history, dimmed. |

Fetching follows `docs/architecture/history/history.md` ("The app"): `/history` when a detail screen opens and every 60 s while open (ETag, so mostly 304); `/history/<id>` only for an http/tcp monitor's screen. Nothing in the list, background refresh or widget fetches history. A row with no prober and no host shows no `›`.

## Files

| file | what |
|---|---|
| `monitor.html`, `check.html`, `states.html` | The pages. `?m=light` for light, `?f=wide` for the 1440 sheet; monitor.html takes `r=1h\|24h\|30d` and `scrub=<probes ago>`. |
| `history.css` | Layout only, over `../host-detail/beacon.css` (the Theme.swift tokens); no new colour. |
| `fixture.js` | One `/history` body and one `/history/<id>` body, shaped by the two schemas, for preview fixtures. |
| `view.js` | Uptime (the ADR's formula), the strip, the chart and the log. Preview only, no fetching. |
| `shoot.mjs` | Renders every PNG with headless Chrome: `node docs/design/history/shoot.mjs <chrome-headless-shell>` from the repo root. |
