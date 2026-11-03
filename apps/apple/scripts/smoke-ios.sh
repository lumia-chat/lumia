#!/bin/sh
# Smoke-runs the iOS app in a simulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, installs the debug app, launches it
# signed in as that account (the LUMIA_DEV_* environment a debug build
# honours, passed through simctl's SIMCTL_CHILD_ prefix), and screenshots
# each screen the LUMIA_DEV_SCREEN switch can open.
#
#   scripts/smoke-ios.sh            # expects the app built for the simulator (xcodebuild, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-ios.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli="$root/apps/core/target/release/lumia-cli"
app="$here/../Lumia-iOS/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-ios}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 2 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
handle="ios$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Phone smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" send "$channel" "Hello from the CLI" >/dev/null
"$cli" --data-dir "$data" send "$channel" "The iOS app reads this through the same core." >/dev/null
echo "seeded $handle, server $server, channel $channel"

# A booted simulator of that name, or boot one; any iPhone when none has
# that name (the CI runner's are whatever its Xcode ships).
udid=$(xcrun simctl list devices available -j | python3 -c "
import json,sys
all=[d for r in json.load(sys.stdin)['devices'].values() for d in r]
devs=[d for d in all if d['name']=='$simulator'] or [d for d in all if d['name'].startswith('iPhone')]
devs.sort(key=lambda d: d['state']!='Booted')
print(devs[0]['udid'] + ' ' + devs[0]['name'] if devs else '')")
[ -n "$udid" ] || { echo "no iPhone simulator"; exit 1; }
echo "simulator: ${udid#* }"
udid=${udid%% *}
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
xcrun simctl install "$udid" "$app"

shoot() { # <screen> <seconds>
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$handle" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" SIMCTL_CHILD_LUMIA_DEV_SCREEN="$1" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 \
    xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep "$2"
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
  echo "  $1: $work/$1.png"
}
shoot home 8
alive=$(xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep -c "$bundle" || true)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel requests devices new signin link; do shoot "$screen" 6; done
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
[ "$alive" -gt 0 ] || { echo "FAIL: the app was not running after launch"; exit 1; }
echo "smoke-ios: ok (launched signed in, every screen screenshotted under $work)"
