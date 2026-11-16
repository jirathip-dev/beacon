#!/bin/sh
# Runs the real scripts/line-cap.sh on a scratch tree: at the cap it passes, one line over it fails,
# and blank lines, Rust test modules and Swift test targets are not counted.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
t=$(mktemp -d)
trap 'rm -rf "${t:?}"' EXIT
mkdir -p "$t/scripts" "$t/ci" "$t/cli/src/sub" "$t/app/Beacon" "$t/app/BeaconKit/Tests/BeaconKitTests"
cp "$here/line-cap.sh" "$t/scripts/"

printf 'fn a() {}\n\nfn b() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n' > "$t/cli/src/main.rs"
printf 'pub fn c() {}\n' > "$t/cli/src/sub/c.rs"
printf 'let a = 1\n\nlet b = 2\n' > "$t/app/Beacon/A.swift"
printf 'let t1 = 1\nlet t2 = 2\nlet t3 = 3\n' > "$t/app/BeaconKit/Tests/BeaconKitTests/T.swift"

fails=0
expect() {
  printf '[fleet.beacon]\nline_cap = %s\n' "$2" > "$t/ci/cap.toml"
  if sh "$t/scripts/line-cap.sh" > "$t/out" 2>&1; then got=0; else got=$?; fi
  if [ "$got" -eq "$1" ]; then echo "ok: cap $2 -> exit $got ($(cat "$t/out"))"; else
    echo "FAIL: cap $2 -> exit $got, want $1 ($(cat "$t/out"))"; fails=1; fi
}
expect 0 5
expect 1 4
printf '[fleet.beacon]\n' > "$t/ci/cap.toml"
if sh "$t/scripts/line-cap.sh" > /dev/null 2>&1; then echo "FAIL: missing line_cap passed"; fails=1; else echo "ok: missing line_cap fails"; fi
exit "$fails"
