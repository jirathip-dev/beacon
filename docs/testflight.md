# TestFlight: Beacon on your phone (#8)

No Mac needs Xcode. CI archives, signs and uploads the iPhone app (app + widget); TestFlight puts it on the phone.
The recipe is Cooee's (jirathip-dev/cooee `docs/testflight.md`, cooee#144/#180).

## How a build reaches the phone

1. A merge to `main` that touches `app/**` (or **Run workflow** on `apple-testflight` in the Actions tab, from
   `main`) starts `.github/workflows/apple-testflight.yml` on GitHub's macOS runner.
2. The job reads the App Store Connect API key from the `testflight` environment. Only `main` may deploy from that
   environment, so pull request and fork runs can't read the key, and the job itself refuses any other branch.
3. It registers the bundle ids `dev.jirathip.beacon` (app) and `dev.jirathip.beacon.widget` (widget) if they are
   missing and turns on App Groups for both, finds the app and the latest TestFlight build, and sets the build number
   to that build + 1. Nobody bumps it by hand.
4. `xcodebuild archive` and `-exportArchive` sign with `-allowProvisioningUpdates` and the API key. Apple manages
   the distribution certificate and profiles (cloud signing), so there is no `.p12`, no password and no fastlane.
5. The run's summary shows the version, the build number and a TestFlight link. Apple processes the build for a
   few minutes, then the TestFlight app on the phone offers it. If the run fails, its `xcodebuild-log` artifact
   holds the raw log for 7 days, with the key id, issuer id and team scrubbed out.

## The owner's one-time steps

- A `testflight` environment (Settings → Environments → New environment), with deployment branches limited to
  `main`, holding the secrets `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_P8` (the base64 of the `.p8`:
  `base64 -i AuthKey_XXXX.p8 | pbcopy`). The key needs the **Admin** role so it may use cloud-managed distribution
  certificates. If it can't, the run fails and says so.
- The App Store Connect app record exists (created once by hand: Apps → + → New App): platform **iOS**, name
  **Beacon**, primary language **English (U.S.)**, bundle id **dev.jirathip.beacon**, SKU **beacon-ios**, user access
  **Full Access**. If it is ever missing, the run fails and names these fields.
- In the app's TestFlight tab, create an internal testing group, add yourself to it, then install the TestFlight app on
  the phone.
- Only if a run says the App Group can't be set up: the API can turn App Groups on for an id but can't create the
  group itself. Create it once at developer.apple.com → Certificates, Identifiers & Profiles → Identifiers → App
  Groups → + → `group.dev.jirathip.beacon`, then on both `dev.jirathip.beacon` and `dev.jirathip.beacon.widget` tick
  App Groups and assign that group. Re-run the workflow.

The build declares that it uses no non-exempt encryption (only HTTPS), so TestFlight doesn't ask about export
compliance for each build.

## Checking the token script without a key

`scripts/asc-token.swift` signs the App Store Connect token. With a throwaway key it prints a three-part JWT:

```sh
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out /tmp/AuthKey.p8
ASC_KEY_PATH=/tmp/AuthKey.p8 ASC_KEY_ID=TEST ASC_ISSUER_ID=test swift scripts/asc-token.swift
```
