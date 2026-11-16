## Summary

Decision: A host-backed monitor row gets a `›` and opens the host's detail screen. One card shows load, cpu, mem and each disk, each with its value now and a sparkline of the last hour. Below it are the host's checks. Tapping a row opens a chart sheet that starts on 1h and switches to 6h or 24h, shows now, min, avg and max, and lets you hold and drag to read a point. Everything is in Beacon's terminal look, light and dark.
Screens: host detail, chart sheet (1h while scrubbing, 24h), and the states page (entry row, loading, warming up, no metrics, unreachable).
Open questions: (1) Should the detail screen keep the "checks on macmini" card under the metrics? I recommend yes: it is the way back from the row you tapped. (2) The sheet opens on 1h, the same window as the sparkline. Say if you'd rather it open on 24h.

## Images (`png/`, 390 wide at 3×, each `-dark` and `-light`; the `-1440` sheets show both modes side by side)

| image | decision it shows |
|---|---|
| `detail-*.png`, `detail-1440.png` | **Host detail.** The top bar has `[‹ monitors]` and the host name. A `~ host` card marked `last 1h` holds one row per metric: label, value now (bold), a plain line under it, a 132 pt sparkline and `›`. **load**: load1, with `5m · 15m` under it. **cpu**: %. **mem**: % of total, with `used of total GB · swap` under it. **disk `<mount>`**: % used, one row per mount in the host's `disks` order, with `free of size GB free` under it. A mount whose read failed is missing from `disks`, so it gets no row. Under the card: `updated HH:mm · a sample every 60s`. Then `~ checks on <host>`, the host's monitor rows as on the list. |
| (same) | **Sparklines.** One series each, as a line in the accent ink with a faint fill, with a dot on the newest point. Status colours are kept for status. The y range is fixed at 0–100 for %, so a flat disk looks flat. Load runs from 0 to a round number above its peak. A null sample breaks the line (the cpu gap); it is never bridged or drawn as zero. |
| `chart-1h-*.png` | **Chart sheet** (large detent, grab bar, `cpu · macmini`, `[done]`). It has the range keys `[1h] [6h] [24h]`, with the chosen one bold and underlined. The plot is 356 pt wide with a y axis at quarters and time ticks ending in `now`. While you hold and drag, a crosshair and a label `HH:mm value` follow your finger. Under the plot: now, min, avg and max for the range. The footnote gives the sample basis and the count of missing samples. |
| `chart-24h-*.png`, `chart-6h-1440.png` | **Longer ranges** plot the mean of each 1 min (6h) or 4 min (24h) bucket, so the line fits the width. min, avg and max still come from the raw samples, so the 24h max (81.6%) is higher than the plotted peak, as it should be. A bucket with no samples is a gap. |
| `states-*.png`, `states-1440.png` | **Entry**: heartbeat rows whose host has a URL get a mute `›` and are tappable. Other rows (http) stay as they are, with no `›`. **Loading** (first fetch): `↻ reading the lamp · fetching macmini metrics`. **Warming up** (200 with empty arrays): `just lit · no samples yet`. **Not served** (404): Beacon's lighthouse art over `dark lamp · no metrics`, with how to fix it. **Unreachable** with earlier data: an amber `● metrics from 09:31 · timeout after 5s`, the last charts kept, values and lines in mute. With no earlier data it is the same amber line in a worded card. |

Fetching follows the ADR (`docs/architecture/beacon/metrics.md`, "The app"). The screen fetches `/metrics` with If-None-Match when it opens and every 60 s while it is open. A 304 keeps what is drawn. Closing the screen stops fetching. There is no background fetch and no widget change. The text `metrics from` shows the time of the newest sample.

## Files

| file | what |
|---|---|
| `detail.html`, `chart.html`, `states.html` | The pages. Add `?m=light` for light mode, `?f=wide` for the 1440 sheet. chart.html takes `r=1h\|6h\|24h`, `id=load\|cpu\|mem\|disk/…` and `scrub=<minutes ago>`. |
| `beacon.css` | The tokens of `app/Shared/Theme.swift`, copied. The only new one is `--fill`, the accent at 10–13% for the area under a line. |
| `fixture.js` | One `MetricsBody`-shaped body: 1440 samples ending 09:31, with a 03:00 backup, a build 40 min ago and a 4-sample cpu gap. Impl can dump it as a JSON preview fixture. |
| `view.js` | The drawing: sparkline, chart, bucketing and stats. Preview only, with no fetching. |
| `shoot.mjs` | Renders every PNG with headless Chrome and no dependencies: `node docs/design/host-detail/shoot.mjs <chrome-headless-shell>` from the repo root. It fails if a page scrolls sideways. |
