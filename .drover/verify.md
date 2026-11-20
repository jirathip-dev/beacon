# Verify recipe: Beacon (iPhone app + widget)

This recipe covers the iPhone app only, at 390 pt wide (iPhone 17e). There is no web UI, so 1440 does not apply.
The app reads a local fixture config served from this worktree, so it needs no GitHub token and no outside network.
Run every command from the worktree root. Everything it writes goes to `shots/`, which git ignores.

## Launch

```sh
sh .drover/verify/up.sh
```

- What it does:
  - Builds the CLI (`cargo build`) and starts a live `beacon serve --config .drover/verify/live.json --host mini --listen 127.0.0.1:0` of this Mini: one check `alive` (`true`) and real load/cpu/mem/disk `/` metrics. It runs with `BEACON_LAUNCHCTL=true`, so it discovers no jobs and the shots are the same on any host. Its pid goes to `shots/live.pid`, its log to `shots/live.log`, and its port is read from the log's first line.
  - Copies `.drover/verify/{monitors.json,status,ok.html}` into `shots/serve/`, with `PORT` replaced by `$DROVER_PORT` and `LIVE` by the live serve's port.
  - Serves them with `.drover/verify/serve.py $DROVER_PORT shots/serve` (a `http.server` that also answers `/history` and `/history/<id>` with bodies built around the current time). Its pid goes to `shots/server.pid` and its log to `shots/server.log`.
  - Builds the `Beacon` scheme (the app, the widget and `BeaconUITests`) with `build-for-testing` into `.build/verify`, with ad-hoc signing so the App Group the widget reads works. The log goes to `shots/build.log`.
  - Creates and boots a throwaway simulator named `beacon-verify-$DROVER_PORT` (its udid goes to `shots/sim.udid`) and installs `Beacon.app`.
- The host has no `sim-task`/`hermes-sim-task`, so the script drives `xcrun simctl` directly. It touches only the simulator it created.
- Ready signal: the last line is `READY sim=<udid> config=http://127.0.0.1:<port>/monitors.json live=http://127.0.0.1:<live>/metrics`, and the exit code is 0.
- Each `up.sh` run deletes the old `shots/*.png` first, so every screenshot comes from the run in hand.
- Teardown: `sh .drover/verify/down.sh` prints `DOWN`. It stops the fixture server and the live serve, and shuts down and deletes that one simulator. It is safe to run twice, and `up.sh` runs it first.
- The app is pointed at the fixtures by a launch argument, with no UI typing. The form only accepts https, but the `source` default is read from the argument domain:
  `xcrun simctl launch "$(cat shots/sim.udid)" dev.jirathip.beacon -source "<$(printf '{"url":{"_0":"http://127.0.0.1:%s/monitors.json"}}' "$DROVER_PORT" | xxd -p | tr -d '\n')>"`.
  Without `-source`, the app opens on first run, because nothing is saved until a config loads through the form.

## Doctor

```sh
curl -fsS "http://127.0.0.1:$DROVER_PORT/monitors.json" -o /dev/null && curl -fsS "http://127.0.0.1:$DROVER_PORT/status" -o /dev/null && xcrun simctl get_app_container "$(cat shots/sim.udid)" dev.jirathip.beacon >/dev/null && echo DOCTOR OK
```

It prints `DOCTOR OK` and exits 0 when both fixture files are served and the app is installed on the lane's simulator.

## Drive

`app/BeaconUITests/VerifyUITests.swift` holds the taps. It runs from the build above, and each test writes `<screen>.png` (1170 px, 3×) into `shots/`:

```sh
TEST_RUNNER_SHOTS="$PWD/shots" TEST_RUNNER_PORT="$DROVER_PORT" xcodebuild test-without-building -project app/Beacon.xcodeproj -scheme Beacon -destination "id=$(cat shots/sim.udid)" -derivedDataPath .build/verify -only-testing:BeaconUITests/VerifyUITests >shots/drive.log 2>&1; echo "drive exit=$?"
```

To drive one screen, use `-only-testing:BeaconUITests/VerifyUITests/<test>` (see the Feature map). The selectors these tests use, and which a new test can reuse:

- App (`dev.jirathip.beacon`):
  - `staticTexts["config url"]`: the first-run form.
  - `staticTexts["backup.ok is 31h old"]`: the first check has landed.
  - `buttons["[cfg]"]`: opens settings.
  - `staticTexts["fetched"]`: the settings source card.
  - `buttons["[done]"]`: closes settings.
  - `buttons["[connect]"]`: the form's only action.
- SpringBoard (`com.apple.springboard`):
  - Press Home until the app's state is `runningBackground`, then long-press `icons["Beacon"]` → `buttons["Edit Home Screen"]`, then `buttons["Edit"]` → `buttons["Add Widget"]`.
  - Every SpringBoard step waits for its element to exist, and each add waits for the widget count to grow (no fixed sleeps).
  - The app's notification alert never shows while the app is in front: iOS shows it over SpringBoard later, at any step. Each wait taps its `Allow` when it shows.
  - `searchFields.firstMatch` and type `Beacon`, then `cells["Beacon"]`.
  - The size preview is a button whose value ends in `Small`; swipe left for Medium.
  - Then the button whose label ends in `Add Widget`, then `buttons["Done"]`.
  - Placed widgets are `icons` with identifier `Beacon` and a value starting with `Widget`.
- Manual look: `xcrun simctl io "$(cat shots/sim.udid)" screenshot shots/<name>.png` after a `simctl launch` (see Launch).
- Appearance: `xcrun simctl ui "$(cat shots/sim.udid)" appearance dark|light`.

The fixture config (`.drover/verify/monitors.json`, 9 monitors in 4 groups) plus the three job rows of the fixture `/status` give `6 down  6 up`:

| group | monitor | fixture | expected row |
|---|---|---|---|
| web | Status page | `GET /ok.html` 200, has `beacon ok` | up |
| web | API wrong status | `GET /health` → 404, expects 200 | down `HTTP 404` |
| web | Keyword missing | `/ok.html` lacks `Example Domain` | down `keyword missing: Example Domain` |
| hosts | Fixture port | TCP 127.0.0.1:`$DROVER_PORT` | up |
| hosts | Closed port | TCP 127.0.0.1:1, refused | down `The operation couldn't be completed. (Network.N…` |
| macmini | Disk space | heartbeat `disk` in `/status`: up | up `since 12 Nov` |
| macmini | Nightly backup | stale heartbeat: `backup` down `backup.ok is 31h old` since 2026-11-12 | down `since 12 Nov` |
| macmini | Photos sync | id `photos` missing from `/status` | down `no result yet` |
| mini | Live serve | heartbeat `alive` from the live `beacon serve` | up, since the serve started |
| macmini | dev.example.tick | kept job row (`name` and `group`) in `/status` | up `last exit 0`, after the macmini heartbeats |
| Unreviewed | com.example.sync | job row with a `name` and no `group` | down `last exit 1` |
| Unreviewed | com.example.ttyd | job row with a `name` and no `group` | up `running` |

The Unreviewed group is the last card. Its rows and the kept job row open `macmini`'s host detail, like its heartbeat rows.

Heartbeat rows (macmini, mini) end in a mute `›` and open their host's detail. `macmini` serves no `/metrics` (404), so its detail is the "dark lamp · no metrics" state; `mini` shows live charts.

## Evidence

- Screenshots: native shots are 1170 px wide (390 pt at 3×). Make the 390 px copies with
  `for f in first-run list unreviewed settings widgets host-detail chart-1h chart-24h host-not-served monitor-history monitor-1h monitor-30d host-checks check-history; do sips --resampleWidth 390 "shots/$f.png" --out "shots/$f-390.png" >/dev/null; done`.
  `shots/widget-small.png` and `shots/widget-medium.png` are the widgets cropped from the home screen at native scale.
- Transcript: `shots/drive.log` (xcodebuild, with every `Test Case … passed|failed` line), `shots/build.log`, `shots/server.log` (each GET the app and widget made, with its status) and `shots/live.log` (the live serve).
- Exit codes to record: `up.sh` (0 and `READY`), Doctor (0 and `DOCTOR OK`), the Drive xcodebuild (`drive exit=0`, `** TEST SUCCEEDED **`), and `down.sh` (0 and `DOWN`).
- All files stay in this worktree's `shots/`.

## Feature map

| screen / feature | how to reach it | test | shot |
|---|---|---|---|
| First run (config form) | launch with no `-source` | `testFirstRun` | `first-run.png` |
| Monitor list (grouped cards, summary, down rows) | launch with `-source` (see Launch) | `testListAndSettings` | `list.png` |
| Job rows: the kept job and the Unreviewed group | list → swipe up to `com.example.sync` | `testListAndSettings` | `unreviewed.png` |
| Host detail (load/cpu/mem/disk sparklines, checks on the host), live | list → tap `Live serve` | `testHostDetailAndChart` | `host-detail.png` |
| Chart sheet (1h, then 24h) | host detail → tap `cpu` → `[24h]` | `testHostDetailAndChart` | `chart-1h.png`, `chart-24h.png` |
| Host with no `/metrics` (404) | list → tap `Disk space` | `testHostDetailAndChart` | `host-not-served.png` |
| Monitor history (uptime, 30-day strip, response time 24h, incidents), `prober: macmini` | list → tap `Status page` | `testHistory` | `monitor-history.png` |
| Response time 1h and 30d | monitor history → `[1h]`, then `[30d]` | `testHistory` | `monitor-1h.png`, `monitor-30d.png` |
| Check rows with `›` on the host detail | list → tap `Nightly backup` | `testHistory` | `host-checks.png` |
| Check history (open incident on top, no chart) | host detail → tap `Nightly backup` | `testHistory` | `check-history.png` |
| Chart scrub (hold and drag), unreachable and warming-up states | hold and drag on the chart; stop the live serve | (manual) | none |
| Settings sheet (`config`, source card) | list → tap `[cfg]` | `testListAndSettings` | `settings.png` |
| Pull to refresh | list → swipe down on the scroll view | (manual) | none |
| Small and medium widget | list once, then Home → long-press the Beacon icon → Edit Home Screen → Edit → Add Widget → Beacon (Small, then swipe to Medium) | `testWidgets` | `widgets.png`, `widget-small.png`, `widget-medium.png` |
| Offline state (`no network · N unknown`) | not reachable on the simulator: it always has a network path | none | none |
| Background refresh and local alerts | `BGAppRefreshTask`: not triggerable from `simctl` | none | none |

## Known noise

- `server.log` shows `code 404, message File not found` / `"GET /health HTTP/1.1" 404`. This is intended: the `API wrong status` fixture.
- `server.log` also shows `"GET /metrics HTTP/1.1" 404`. This is intended: `macmini` is a serve from before metrics.
- The Closed port row's error is iOS's own `NWError` text, cut off with an ellipsis.
- `beacon check shots/serve/monitors.json` (the CLI) rejects the fixture because its URLs are http. The fixture is for the app only, which accepts http to 127.0.0.1.
- `testWidgets` takes about 3 minutes, and `drive.log` shows `App animations complete notification not received` twice: SpringBoard never reports idle while its icons jiggle, so XCUITest waits 60 s before each of two taps and then goes on.
- The home screen shows a `BeaconUITests-Runner` icon. That is the XCUITest runner app.
- The `since` times are the run's clock time, except the heartbeat rows (`12 Nov`, from the fixture's `since`).
