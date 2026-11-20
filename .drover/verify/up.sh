#!/bin/sh
# Builds the app and its UI tests at HEAD, serves the fixtures on $DROVER_PORT, boots a throwaway iPhone 17e
# (390 pt) with the app installed, and runs a live `beacon serve` of this Mini (host `mini`, metrics) on a free port.
# State goes in shots/; .drover/verify/down.sh undoes all of it.
set -eu
: "${DROVER_PORT:?}"
cd "$(dirname "$0")/../.."
sh .drover/verify/down.sh
mkdir -p shots/serve
rm -f shots/*.png
cargo build -q --manifest-path cli/Cargo.toml
BEACON_LAUNCHCTL=true "${CARGO_TARGET_DIR:-cli/target}/debug/beacon" serve --config .drover/verify/live.json --host mini --listen 127.0.0.1:0 >shots/live.log 2>&1 &
echo $! >shots/live.pid
live=
for _ in 1 2 3 4 5 6 7 8 9 10; do
  live=$(sed -n 's/^serve mini on [0-9.]*:\([0-9]*\):.*/\1/p' shots/live.log)
  [ -n "$live" ] && break
  sleep 1
done
test -n "$live" || { echo "beacon serve did not start: $(cat shots/live.log)" >&2; exit 1; }
for f in monitors.json status ok.html; do
  sed -e "s/PORT/$DROVER_PORT/g" -e "s/LIVE/$live/g" ".drover/verify/$f" >"shots/serve/$f"
done
python3 .drover/verify/serve.py "$DROVER_PORT" shots/serve >shots/server.log 2>&1 &
echo $! >shots/server.pid
xcodebuild -project app/Beacon.xcodeproj -scheme Beacon -sdk iphonesimulator -derivedDataPath .build/verify \
  CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= build-for-testing >shots/build.log 2>&1
xcrun simctl create "beacon-verify-$DROVER_PORT" com.apple.CoreSimulator.SimDeviceType.iPhone-17e >shots/sim.udid
sim=$(cat shots/sim.udid)
xcrun simctl boot "$sim"
xcrun simctl bootstatus "$sim" >/dev/null
xcrun simctl install "$sim" .build/verify/Build/Products/Debug-iphonesimulator/Beacon.app
curl -fsS "http://127.0.0.1:$DROVER_PORT/monitors.json" >/dev/null
curl -fsS "http://127.0.0.1:$live/metrics" >/dev/null
echo "READY sim=$sim config=http://127.0.0.1:$DROVER_PORT/monitors.json live=http://127.0.0.1:$live/metrics"
