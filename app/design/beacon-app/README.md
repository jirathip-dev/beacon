# Beacon app design (issue #3)

Preview-only SwiftUI views and fixtures (`Sources/`): no BeaconKit, no networking, no app logic.
`./render.sh` builds `BeaconDesign.xcodeproj` and runs each screen on a throwaway iPhone 17e simulator
(390 pt wide, PNGs at 3× = 1170 px), then deletes the simulator. It also writes the widgets with
`ImageRenderer` at the 390 pt widget sizes (small 158×158, medium 338×158). Every screen also has an
Xcode `#Preview`. **The owner picks look A or B.** Impl builds only that look, with no theme picker.

Fixture: 7 monitors in 4 groups taken from `docs/monitors.example.json` plus a `nas` host. Three are up,
and four are down: `http` 503, a heartbeat's own failure, a heartbeat whose host is unreachable, and a
missing check id. The last config fetch failed.

## Shared decisions (both looks)

- **One screen is the app**: the grouped monitor list. Rows are not tappable (q1580), and there is no
  detail screen. Pull-to-refresh is on the list.
- **Row**: status, then the name. The error text sits on a second line, only when down. "since <t>" is
  trailing, with `HH:mm` for today and `MMM d` for earlier days. Unknown rows show no since.
- **"config from <t>"** appears at the top of the list only when the config fetch failed (D2).
- **Offline** (`NWPathMonitor` unsatisfied): every row is grey, with no detail or since, and one
  "No network" line at the top. Nothing turns red (D5).
- **First run = Settings form** (q1578): config URL, an optional header name, and a header value
  (secure, Keychain), then **Load** and **Import from Files**. An invalid file or URL body shows the
  decode error inline under the buttons. Settings is the same form in a sheet from the list, with the
  current source and header shown first.
- **Widgets** (D8) show only what `StatusSnapshot` holds: the down count, the up count, the names that
  are down (small: 3, then "+N more"; medium: 5), and the last check time.

## Look A: plain iOS (system list, SF Symbols, indigo accent, light + dark)

| PNG | Shows |
|---|---|
| `png/A-first-run.png` | First run: URL, header, Load / Import from Files |
| `png/A-first-run-error.png` | Decode error of a refused import, in red under the buttons |
| `png/A-list.png` | Grouped inset list; green/red dots; error as secondary second line; "config from 08:40" line with `exclamationmark.icloud` |
| `png/A-list-dark.png` | The same in dark mode (system colours only) |
| `png/A-list-offline.png` | All dots grey, "No network" with `wifi.slash` |
| `png/A-settings.png`, `png/A-settings-dark.png` | Settings sheet: Current (source, fetched time, header name masked), then the first-run controls, Done |
| `png/A-widget-small.png`, `-dark` | ✕ "4 down", "3 up", three names + "+1 more", time |
| `png/A-widget-small-all-up.png`, `-dark` | ✓ "All up", "7 up" |
| `png/A-widget-medium.png`, `-dark` | Big red down count + up count + time on the left; down names with red dots on the right |

## Look B: terminal (dark only, SF Mono 13, dense rows)

| PNG | Shows |
|---|---|
| `png/B-first-run.png` | `beacon` top bar; boxed fields; `[ load ]` (dim until a URL) `[ import from files ]` |
| `png/B-first-run-error.png` | `error: <decode error>` in red |
| `png/B-list.png` | Summary line `4 down  3 up`, `config from 08:40`; `# group` headers; status words `up` / `DOWN` / `--` in green/red/grey; error indented in red; `[cfg]` opens settings |
| `png/B-list-offline.png` | `no network: 7 unknown`, every row `--` grey |
| `png/B-settings.png` | Settings sheet: `source / fetched / header` lines, then the same fields, `[done]` |
| `png/B-widget-small.png` | `4 down` / `3 up`, `x API` … `+1`, `@ 09:31` |
| `png/B-widget-small-all-up.png` | `0 down` (grey) / `7 up` |
| `png/B-widget-medium.png` | Counts and time on the left; `DOWN <name>` list on the right |
