# Beacon app design (issue #3)

The owner picked **look B (terminal)** and dropped look A (q1584). B now uses Cooee's terminal style
(`cooee web/src/app.css` `:root` + `html[data-style=terminal]`). Impl builds only this look, with no theme picker.

Preview-only SwiftUI views and fixtures (`Sources/`): no BeaconKit, no networking, no app logic.
`./render.sh` builds `BeaconDesign.xcodeproj` and runs each screen in light and dark on a throwaway
iPhone 17e simulator (390 pt wide, PNGs at 3× = 1170 px), then deletes the simulator. It also writes the
widgets with `ImageRenderer` at the 390 pt widget sizes (small 158×158, medium 338×158). Every screen also has an
Xcode `#Preview`.

Fixture: 7 monitors in 4 groups taken from `docs/monitors.example.json` plus a `nas` host. Three are up,
and four are down: `http` 503, a heartbeat's own failure, a heartbeat whose host is unreachable, and a
missing check id. The last config fetch failed. One name and one error are long, to show the ellipsis.

## Tokens (`T` in `Sources/Terminal.swift`)

| token | dark | light | use |
|---|---|---|---|
| bg | `#0f1a13` | `#f5efda` | screen, widget |
| card | `#1d1a12` | `#ebe2c4` | group cards, fields |
| edge | `#3b3a27` | `#cbbd90` | 1 pt card and field border |
| line | `#26301f` | `#e0d6b4` | row dividers, top-bar rule |
| fg | `#ece4cc` | `#29251a` | names, values |
| mute | `#a3a68a` | `#5c573f` | since, errors, labels, unknown |
| accent | `#ecdba6` | `#6b3410` | `[keys]`, settings card rule |
| green | `#7fd06a` | `#2a6d1c` | up |
| amber | `#f29b4b` | `#7a5f00` | config from, no network |
| red | `#ff7d6e` | `#b81d3c` | down |

The light edge, line and amber are Cooee's own light values (the q1584 note did not list them). The app follows the
System light/dark setting. Type: SF Mono 13 pt (meta 11 pt, titles 15 pt bold). Cards and fields have a 6 pt radius.

## Decisions

- **One screen is the app**: the grouped monitor list. Rows are not tappable (q1580), and there is no detail
  screen. Pull-to-refresh is on the list.
- **Group = card** with a 1 pt edge and a 3 pt coloured top rule: red if any row is down, green if all are up,
  and mute if all are unknown. The header is `~ name` plus a count: `2 down` in red, else `N up` or `N unknown`
  in mute.
- **Row**: a status dot, the name (one line, ellipsis), then a trailing `up|down|-- since <t>`. The word takes
  the status colour and `since` is mute (`HH:mm` today, `MMM d` earlier; no since when unknown). A down row adds
  its error on a second line (one line, ellipsis, mute) and a 3 pt red left rule.
- **Summary line** at the top: `4 down  3 up` (bold). Under it, `● config from 08:40 · fetch failed` in amber,
  shown only when the config fetch failed (D2).
- **Offline** (`NWPathMonitor` unsatisfied): `● no network · 7 unknown` in amber. Every dot, word and top rule
  is grey. Nothing turns red (D5).
- **Keys** are lowercase `[bracket]` words in the accent colour, each with a ≥44×44 pt hit area. This fixes
  the small `[cfg]` tap target.
- **Worded states** follow Cooee's `phrase · plain words` form in a centred card. Loading (config in, first
  check pending): `↻ lighting up · checking 7 monitors`. Empty (config has no monitors):
  `dark coast · no monitors` with `[change config]`.
- **First run = Settings form** (q1578): config URL, an optional header name, and a header value (secure,
  Keychain), then `[load]` (mute until there is a URL) and `[import from files]`. A refused file or URL body shows
  `● refused <decode error>` under the keys, in a card with a red left rule. Settings is the same form in a sheet
  from `[cfg]`, titled `config` with `[done]`. The current source, fetched time and masked header come first, in
  a card with an accent rule.
- **Widgets** (D8) use the same palette in both modes and show only what `StatusSnapshot` holds: the down count
  (or `all up`), the up count, the down names with red dots (small: 3, then `+N more`; medium: 5), and
  `checked <t>`.

## PNGs (`png/`, each `-light` and `-dark`)

| PNG | Shows |
|---|---|
| `first-run-*.png` | First run: three fields, `[load]` dim (no URL yet), `[import from files]` |
| `first-run-error-*.png` | `● refused monitors[2]: unknown type "ping"` card with a red rule |
| `list-*.png` | Summary, config-from line, four group cards with red/green top rules and counts, down rows with a red left rule, ellipsised name and error |
| `list-offline-*.png` | `no network · 7 unknown`; all rows `--`, all rules grey |
| `list-loading-*.png` | `↻ lighting up · checking 7 monitors` |
| `list-empty-*.png` | `dark coast · no monitors` + `[change config]` |
| `settings-*.png` | `config` sheet: current-source card, the same fields filled in, `[done]` |
| `widget-small-*.png` | `4 down` / `3 up`, three red-dot names + `+1 more`, `checked 09:31` |
| `widget-small-all-up-*.png` | `all up` / `7 up` |
| `widget-medium-*.png` | `beacon`, counts and time on the left; down names on the right of a divider |
