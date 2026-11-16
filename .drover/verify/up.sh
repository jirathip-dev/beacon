#!/bin/sh
# Builds the app and its UI tests at HEAD, serves the fixtures on $DROVER_PORT, boots a throwaway iPhone 17e
# (390 pt) with the app installed. State goes in shots/; .drover/verify/down.sh undoes all of it.
set -eu
: "${DROVER_PORT:?}"
cd "$(dirname "$0")/../.."
sh .drover/verify/down.sh
mkdir -p shots/serve
rm -f shots/*.png
for f in monitors.json status ok.html; do sed "s/PORT/$DROVER_PORT/g" ".drover/verify/$f" >"shots/serve/$f"; done
python3 -m http.server "$DROVER_PORT" --bind 127.0.0.1 --directory shots/serve >shots/server.log 2>&1 &
echo $! >shots/server.pid
xcodebuild -project app/Beacon.xcodeproj -scheme Beacon -sdk iphonesimulator -derivedDataPath .build/verify \
  CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= build-for-testing >shots/build.log 2>&1
xcrun simctl create "beacon-verify-$DROVER_PORT" com.apple.CoreSimulator.SimDeviceType.iPhone-17e >shots/sim.udid
sim=$(cat shots/sim.udid)
xcrun simctl boot "$sim"
xcrun simctl bootstatus "$sim" >/dev/null
xcrun simctl install "$sim" .build/verify/Build/Products/Debug-iphonesimulator/Beacon.app
curl -fsS "http://127.0.0.1:$DROVER_PORT/monitors.json" >/dev/null
echo "READY sim=$sim config=http://127.0.0.1:$DROVER_PORT/monitors.json"
