#!/bin/sh
# Preferences by hand, in a simulator against a dev instance: what
# smoke-ios.sh cannot show, since it only launches and looks. The UI test
# (LumiaUITests/SmokeUITests/testPreferencesAreChosenAndKept) opens the
# account menu on the rail's own avatar, goes to Preferences, reads each
# choice as assistive technology does, chooses Dark and Minimal, then font
# size 18 and Cozy, which leaves no preset selected and says Custom,
# launches again on the channel with nothing named (one line, cozy, at 18,
# each message still one stop that says whose and when), and then chooses
# System, Comfortable and 15 back. This script seeds the account and looks
# at the screen at each step: dark once Dark is chosen, dark at the next
# launch, and the simulator's own appearance again once System is.
# Screenshots land in .build/smoke-preferences/. It is not in the macOS
# workflow yet.
#
#   scripts/smoke-preferences.sh     # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-preferences.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-preferences.sh   # a CLI built elsewhere (another worktree)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-preferences}
rm -rf "$work" && mkdir -p "$work/steps"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
handle="prefs$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Preferences smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
for words in "One." "Two, a moment later." "Three, under the same name."; do "$cli" --data-dir "$data" send "$channel" "$words" >/dev/null; done
echo "seeded $handle, server $server, channel $channel"

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
# Uninstalling takes the app's own defaults, where the choices are kept, so
# the test starts as a new phone does; the app group's container keeps the
# stores across installs, so that goes too.
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
# What System is to follow.
phone=$(xcrun simctl ui "$udid" appearance 2>/dev/null || echo light)

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT

TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testPreferencesAreChosenAndKept > "$work/uitest.log" 2>&1 &
uitest=$!

# Waits for the test to announce a step and screenshots it; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    echo "FAIL: the UI test did not reach \"$1\"; see $work/uitest.log"
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10
    exit 1
  fi
  sleep 1 # the change's own fade
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
}

# Whether a screenshot is of a dark screen or a light one, by the app's
# background beside the status bar's clock (as a bitmap, whose pixels are
# there to read: rows from the bottom, blue first).
looks() { # <picture>
  sips -s format bmp "$work/$1.png" --out "$work/$1.bmp" >/dev/null 2>&1
  python3 - "$work/$1.bmp" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
start, = struct.unpack("<I", data[10:14])
width, height, _, bits = struct.unpack("<iiHH", data[18:30])
step = bits // 8
stride = (width * step + 3) // 4 * 4
x, y = width // 50, abs(height) // 20
row = y if height < 0 else height - 1 - y
at = start + row * stride + x * step
b, g, r = data[at], data[at + 1], data[at + 2]
print("dark" if (r + g + b) / 3 < 128 else "light")
PY
  rm -f "$work/$1.bmp"
}
expect() { # <picture> <dark|light> <what it shows>
  is=$(looks "$1")
  [ "$is" = "$2" ] || { echo "FAIL: $3 is $is, not $2; see $work/$1.png"; exit 1; }
}

reached menu 240
echo "the account menu on the rail's avatar has View profile, Preferences and Devices"
reached dark 60
expect dark dark "Preferences with Dark chosen"
echo "Dark and Minimal are chosen, then 18 and Cozy, which is Custom; each read as selected, and the screen is dark"
reached kept 120
expect kept dark "the channel at the next launch"
echo "the next launch draws the channel dark and in one line, with nothing naming the choices"
reached system 120
expect system "$phone" "Preferences with System chosen"
echo "System, Comfortable and 15 are chosen back, and the screen is the simulator's $phone again"
wait "$uitest" || { echo "FAIL: the UI test did not pass; see $work/uitest.log"; grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10; exit 1; }
uitest=""
echo "smoke-preferences: ok (the account menu leads to Preferences, each choice says whether it is selected, takes effect as it is made and is kept for the next launch; screenshots under $work)"
