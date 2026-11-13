Closes #1

## For humans
- The repo now has its skeleton: a command-line tool that checks a monitors config file, and an iPhone app with a home-screen widget that both build.
- Every pull request runs the Rust checks, the iPhone build and tests, and a size limit on the source code.
- `docs/monitors.example.json` is the reference config. Both the tool and the app test it.

## What changed
- `cli/`: Rust crate `beacon` (deps: serde and serde_json only). `config.rs` parses `monitors.json` v1 strictly (`deny_unknown_fields`). It enforces the schema's bounds plus the cross-field rules from `types.md` (a heartbeat names an existing host check, check ids are unique per host, `timeout_s < every_s`). URLs must be https:// (q1547). `beacon check <path>` exits 0 when valid, or 2 with the reason.
- `app/`: `Beacon.xcodeproj` (SwiftUI app plus a WidgetKit extension, embedded), linked to the local `BeaconKit` package. BeaconKit decodes the config leniently (q1511): it ignores unknown keys and throws on an unknown version or monitor type.
- `docs/monitors.example.json` moved here from the ADR folder, as the ADR README says. The schema's url patterns are now `^https://` (amendment q1547).
- CI (`.github/workflows/ci.yml`) has three jobs. `rust` runs fmt --check, clippy -D warnings and test. `line-cap` runs the script and its test. `swift` runs xcodebuild build of the app and xcodebuild test of BeaconKit on an iPhone simulator.
- `scripts/line-cap.sh` counts non-blank lines in `cli/src` before `#[cfg(test)]` and in `app/**/*.swift` outside `*Tests/`. `scripts/line-cap.test.sh` runs the real script on a scratch tree: at the cap it passes, one over it fails, and a missing cap fails.
- `docs/cap-history/2026-11-13-2000.md` and `.herdsman/rules.md` (no rules yet).

## Decisions
- Swift tests run on the BeaconKit package's own scheme. Xcode won't run a local package's test target from the project scheme on the command line (3 scheme variants tried, all "no test bundles"). The app's own scheme is built in the same job.
- The crate is a lib plus a bin, so the config types are public API and need no dead-code allowances. `beacon check <path>` is the smallest real entry point the acceptance test can drive. URL configs come with the issue that adds `ureq`.
- Validation runs host checks before monitors, so a heartbeat error never hides a duplicate check id.

## `ci/cap.toml`
The lane hook refuses to create `ci/cap.toml` (`REFUSED CAP_FILE`). Per decision q1553, the owner committed it on this branch (eaa8ce2):
```toml
[fleet.beacon]
line_cap = 2000
```

## Evidence (at eaa8ce2b198024a772844dbe32ab1e4adf62783a)
- `cd cli && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`: exit 0 (5 integration tests pass)
- `sh scripts/line-cap.sh`: exit 0 (`non-test lines: 318 / 2000`)
- `sh scripts/line-cap.test.sh`: exit 0
- Swift: the code is unchanged since 9e77180, where `xcodebuild ... -scheme Beacon build` and `cd app/BeaconKit && xcodebuild -scheme BeaconKit test` both exited 0 (3 tests pass). CI's `swift` job at eaa8ce2 runs them again: CI_STATUS

src/ net +318 (budget +150). The CLI strictly enforces every bound in the schema and every cross-field rule, which the acceptance test needs. The Swift config types are what D7 has both suites decode.
