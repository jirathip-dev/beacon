Closes #19

## For humans
Pull-to-refresh and the automatic refresh now send the same header the config load used, kept in one place.
When a config fetch really fails, the amber line under the summary now says why (for example "HTTP 404", "timed out", "no header") instead of a bare "fetch failed".
If the phone can't save the header to the Keychain, the load is refused with the Keychain error, so you see it instead of every later refresh failing quietly.

## What changed
- `ConfigStore` owns the header through a `HeaderStore` seam (the app's `Keychain`, a mock in tests). `load` (the form) fetches with the typed header and keeps it once the fetch succeeded. `refresh` (the `Model.run` path) fetches with the kept header. Before, the form wrote the Keychain itself and dropped any `SecItemAdd` failure.
- `LoadedConfig.stale` became `failure: String?`, and `StatusSnapshot.configStale` became `configFailure: String?`. The config is stale exactly when `failure != nil`. A source with a header name but no stored header fails as `no header` without sending a request.
- The stale line shows `· <failure>` in the same muted 11 pt style, on one line.

## Root cause findings
- I probed the real URL, conditional GET with `Authorization: token …` and `If-None-Match`, with curl and with `URLSession` using the app's exact ephemeral, no-cache config. Both returned `304`, and without the header it returned `404`. So the 304 branch and the transport are fine. The difference between the two paths was where the header came from: the form used the text field, while the refresh read the Keychain, whose write failure was silently ignored. The device-side Keychain cause can't be checked from this lane (no device). The load now refuses with `keychain <OSStatus>` if the write fails, and the stale line says `no header` if the read comes back empty.

## Repro
Commit 1 (6946a11) adds only `refreshAfterLoadSendsTheLoadsHeader` and `failedRefreshSaysWhy`. On base they fail to build (exit 65, `cannot find type 'HeaderStore'`). The refresh path lived only in the app target, so BeaconKit had no seam to exercise it.

## Evidence (head 7c3018637f4ffc07150684ffec6456954ba49c4d)
- `cd app/BeaconKit && xcodebuild -scheme BeaconKit -destination "id=<iPhone 18 Pro sim>" test`: exit 65 at 6946a11 (repro), exit 0 at head (19 tests passed)
- `xcodebuild -project app/Beacon.xcodeproj -scheme Beacon -destination "id=<sim>" -derivedDataPath .build/dd CODE_SIGNING_ALLOWED=NO build`: exit 0
- `sh scripts/line-cap.sh`: exit 0 (1923 / 3000)

src/ net +24 (budget +40): a seam protocol plus a `refresh` entry point, with the bool swapped for a reason string.

Not verified here: the 390 screenshots (the verify stage), and the real-device Keychain behaviour.
