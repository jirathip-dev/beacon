# Verify recipe: Beacon (iPhone app + widget)

This recipe covers the iPhone app only, at 390 pt wide (iPhone 17e). There is no web UI, so 1440 does not apply.
The app reads a local fixture config served from this worktree, so it needs no GitHub token and no outside network.
Run every command from the worktree root. Everything it writes goes to `shots/`, which git ignores.

## Launch

```sh
sh .drover/verify/up.sh
```

- What it does:
  - Copies `.drover/verify/{monitors.json,status,ok.html}` into `shots/serve/`, with `PORT` replaced by `$DROVER_PORT`.
  - Serves them with `python3 -m http.server $DROVER_PORT --bind 127.0.0.1`. Its pid goes to `shots/server.pid` and its log to `shots/server.log`.
  - Builds the `Beacon` scheme (the app, the widget and `BeaconUITests`) with `build-for-testing` into `.build/verify`, with ad-hoc signing so the App Group the widget reads works. The log goes to `shots/build.log`.
  - Creates and boots a throwaway simulator named `beacon-verify-$DROVER_PORT` (its udid goes to `shots/sim.udid`) and installs `Beacon.app`.
- The host has no `sim-task`/`hermes-sim-task`, so the script drives `xcrun simctl` directly. It touches only the simulator it created.
- Ready signal: the last line is `READY sim=<udid> config=http://127.0.0.1:<port>/monitors.json`, and the exit code is 0.
- Teardown: `sh .drover/verify/down.sh` prints `DOWN`. It stops the server and shuts down and deletes that one simulator. It is safe to run twice, and `up.sh` runs it first.
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
  - `buttons["[load]"]` and `buttons["[import from files]"]`: the form's actions.
- SpringBoard (`com.apple.springboard`):
  - Long-press at (0.5, 0.65) for edit mode, then `buttons["Edit"]` → `buttons["Add Widget"]`.
  - `searchFields.firstMatch` and type `Beacon`, then `cells["Beacon"]`.
  - The size preview is a button whose value ends in `Small`; swipe left for Medium.
  - Then the button whose label ends in `Add Widget`, then `buttons["Done"]`.
  - Placed widgets are `icons` with identifier `Beacon` and a value starting with `Widget`.
- Manual look: `xcrun simctl io "$(cat shots/sim.udid)" screenshot shots/<name>.png` after a `simctl launch` (see Launch).
- Appearance: `xcrun simctl ui "$(cat shots/sim.udid)" appearance dark|light`.

The fixture config (`.drover/verify/monitors.json`, 8 monitors in 3 groups) gives `5 down  3 up`:

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

## Evidence

- Screenshots: native shots are 1170 px wide (390 pt at 3×). Make the 390 px copies with
  `for f in first-run list settings widgets; do sips --resampleWidth 390 "shots/$f.png" --out "shots/$f-390.png" >/dev/null; done`.
  `shots/widget-small.png` and `shots/widget-medium.png` are the widgets cropped from the home screen at native scale.
- Transcript: `shots/drive.log` (xcodebuild, with every `Test Case … passed|failed` line), `shots/build.log` and `shots/server.log` (each GET the app and widget made, with its status).
- Exit codes to record: `up.sh` (0 and `READY`), Doctor (0 and `DOCTOR OK`), the Drive xcodebuild (`drive exit=0`, `** TEST SUCCEEDED **`), and `down.sh` (0 and `DOWN`).
- All files stay in this worktree's `shots/`.

## Feature map

| screen / feature | how to reach it | test | shot |
|---|---|---|---|
| First run (config form) | launch with no `-source` | `testFirstRun` | `first-run.png` |
| Monitor list (grouped cards, summary, down rows) | launch with `-source` (see Launch) | `testListAndSettings` | `list.png` |
| Settings sheet (`config`, source card) | list → tap `[cfg]` | `testListAndSettings` | `settings.png` |
| Pull to refresh | list → swipe down on the scroll view | (manual) | none |
| Small and medium widget | list once, then Home → Edit → Add Widget → Beacon (Small, then swipe to Medium) | `testWidgets` | `widgets.png`, `widget-small.png`, `widget-medium.png` |
| Offline state (`no network · N unknown`) | not reachable on the simulator: it always has a network path | none | none |
| Background refresh and local alerts | `BGAppRefreshTask`: not triggerable from `simctl` | none | none |

## Known noise

- `server.log` shows `code 404, message File not found` / `"GET /health HTTP/1.1" 404`. This is intended: the `API wrong status` fixture.
- The Closed port row's error is iOS's own `NWError` text, cut off with an ellipsis.
- `beacon check shots/serve/monitors.json` (the CLI) rejects the fixture because its URLs are http. The fixture is for the app only, which accepts http to 127.0.0.1.
- The home screen shows a `BeaconUITests-Runner` icon. That is the XCUITest runner app.
- The `since` times are the run's clock time, except the heartbeat rows (`12 Nov`, from the fixture's `since`).
