#!/bin/sh
# Renders every design screen to png/ on a throwaway iPhone 17e (390 pt wide) simulator, then deletes it.
set -eu
cd "$(dirname "$0")"
dd=/tmp/beacon-design-dd
out="$PWD/png"
mkdir -p "$out"
xcodebuild -project BeaconDesign.xcodeproj -scheme BeaconDesign -sdk iphonesimulator \
  -derivedDataPath "$dd" CODE_SIGNING_ALLOWED=NO build >/tmp/beacon-design-build.log
app="$dd/Build/Products/Debug-iphonesimulator/BeaconDesign.app"
sim=$(xcrun simctl create beacon-design-render com.apple.CoreSimulator.SimDeviceType.iPhone-17e)
trap 'xcrun simctl shutdown "$sim" >/dev/null 2>&1 || true; xcrun simctl delete "$sim"' EXIT
xcrun simctl boot "$sim"
xcrun simctl bootstatus "$sim" >/dev/null
xcrun simctl status_bar "$sim" override --time 9:41 --batteryState charged --batteryLevel 100 \
  --cellularBars 4 --wifiBars 3
xcrun simctl install "$sim" "$app"
shot() { # shot <screen> <png name> [dark]
  [ "${3:-}" = dark ] && xcrun simctl ui "$sim" appearance dark || xcrun simctl ui "$sim" appearance light
  SIMCTL_CHILD_SCREEN="$1" SIMCTL_CHILD_OUT="$out" \
    xcrun simctl launch --terminate-running-process "$sim" dev.jirathip.beacon.design >/dev/null
  sleep 3
  xcrun simctl io "$sim" screenshot "$out/$2.png" >/dev/null 2>&1
}
for s in first-run first-run-error list list-offline settings; do
  shot "A-$s" "A-$s"
  shot "B-$s" "B-$s" dark
done
shot A-list A-list-dark dark
shot A-settings A-settings-dark dark
echo "wrote $(ls "$out" | wc -l | tr -d ' ') pngs to $out"
