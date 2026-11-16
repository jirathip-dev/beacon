#!/bin/sh
# Stops the fixture server and deletes the simulator that up.sh started; a no-op when nothing runs.
cd "$(dirname "$0")/../.."
[ -f shots/server.pid ] && kill "$(cat shots/server.pid)" 2>/dev/null
[ -f shots/sim.udid ] && xcrun simctl shutdown "$(cat shots/sim.udid)" 2>/dev/null
[ -f shots/sim.udid ] && xcrun simctl delete "$(cat shots/sim.udid)"
rm -f shots/server.pid shots/sim.udid
echo DOWN
