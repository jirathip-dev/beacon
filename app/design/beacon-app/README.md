## Summary

Decision: no token anywhere. The start page and cfg are one `config url` field, prefilled with the Mini's `/config`, plus `[connect]`. A failed connect says why in plain words on the red refused row. cfg puts a card under the form with the source host, the fetched time and the last error (#32).
Screens: start page (empty, connecting, refused) and cfg (connected, last error), at 390 pt, light and dark. The header fields and `[import from files]` are gone.
Open questions: none. On upgrade, the start page opens as a sheet over the cached list (no `[done]`), so the old rows stay visible until a connect works.

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
- **Checked time** (#20): the summary line ends with `checked HH:MM` (`StatusSnapshot.at` via `T.stamp`) in mute
  11 pt, the same style as `· fetch failed`. It moves on every refresh. Offline keeps its line and adds the same
  stamp: `● no network · 7 unknown  checked 09:31`. No stamp before the first check.
- **Offline** (`NWPathMonitor` unsatisfied): `● no network · 7 unknown` in amber. Every dot, word and top rule
  is grey. Nothing turns red (D5).
- **Keys** are lowercase `[bracket]` words in the accent colour, each with a ≥44×44 pt hit area. This fixes
  the small `[cfg]` tap target.
- **Worded states** follow Cooee's `phrase · plain words` form in a centred card. Loading (config in, first
  check pending): `↻ lighting up · checking 7 monitors`. Empty (config has no monitors):
  `dark coast · no monitors` with `[change config]`.
- **Start page** (#32, q1890, q1895, replaces the q1578 form): one `config url` field, prefilled with
  `https://jirathips-mac-mini.tail8c3301.ts.net/config`, and `[connect]`. The field wraps to two lines so the whole URL
  can be read on a phone. There is no header field, no Keychain and no file import. Only https URLs are accepted.
- **Connecting**: `[connect]` turns mute and can't be tapped, the URL dims, and `↻ connecting · reaching the Mini`
  shows beside the key in the 11 pt meta style.
- **Refused**: the existing card with a red left rule, `● refused <reason>`. The reason is in plain words, up to
  two lines: `can't reach the Mini: is Tailscale on?`, `HTTP 404`, `not a Beacon config`, `config url must be https`.
- **cfg** is the same form in a sheet from `[cfg]`, titled `config` with `[done]`. The field holds the current URL.
  Under `[connect]` is a card with an accent rule: `source` (the URL's host, middle-ellipsised), `fetched` (`HH:MM`)
  and `last error` (red, or `none` in mute). A failed connect in cfg shows the same refused row above the card.
- **Upgrade**: a saved source with a header, or the `.local` source, is dropped. The start page (prefilled) then opens
  as a sheet that can't be dismissed, over the cached list, until a connect works.
- **Widgets** (D8) use the same palette in both modes and show only what `StatusSnapshot` holds: the down count
  (or `all up`), the up count, the down names with red dots (small: 3, then `+N more`; medium: 5), and
  `checked <t>`.

## PNGs (`png/`, each `-light` and `-dark`)

| PNG | Shows |
|---|---|
| `start-*.png` | Start page: the `config url` field prefilled with the Mini's URL, `[connect]` (#32) |
| `start-connecting-*.png` | `[connect]` mute, URL dimmed, `↻ connecting · reaching the Mini` |
| `start-refused-*.png` | `● refused can't reach the Mini: is Tailscale on?` card with a red rule |
| `list-*.png` | Summary `4 down  3 up  checked 09:31` (#20), config-from line, four group cards with red/green top rules and counts, down rows with a red left rule, ellipsised name and error |
| `list-offline-*.png` | `no network · 7 unknown  checked 09:31` (#20); all rows `--`, all rules grey |
| `list-loading-*.png` | `↻ lighting up · checking 7 monitors` |
| `list-empty-*.png` | `dark coast · no monitors` + `[change config]` |
| `cfg-*.png` | `config` sheet: the URL and `[connect]`, then a card with `source` (the Mini's host), `fetched 09:31`, `last error none` |
| `cfg-error-*.png` | The same card with `fetched 08:40` and `last error HTTP 404` in red (the list behind keeps its amber stale line, #19) |
| `widget-small-*.png` | `4 down` / `3 up`, three red-dot names + `+1 more`, `checked 09:31` |
| `widget-small-all-up-*.png` | `all up` / `7 up` |
| `widget-medium-*.png` | `beacon`, counts and time on the left; down names on the right of a divider |
