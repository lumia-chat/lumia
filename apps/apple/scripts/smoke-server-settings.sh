#!/bin/sh
# Server settings by hand (architecture 6.7), in a simulator against a dev
# instance. The phone signs in as a server's owner, renames the server and
# sets how long it keeps messages through the screen
# (LumiaUITests/SmokeUITests/testServerSettingsAreChangedHere), and this
# script reads both back from the instance: the name with the release CLI,
# the retention over HTTP. The screen then turns Show deleted messages on,
# which the script reads back over HTTP. Then it gives the server a picture with the CLI,
# as another of the owner's clients would; the screen and the rail must
# draw it, which is checked by the colour at the middle of the rail's
# tile. Last, the phone signs in as a member the owner invited, who may not
# change the settings and is not offered them
# (testServerSettingsAreNotOfferedToAMember). Screenshots in
# .build/smoke-server-settings/.
#
#   scripts/smoke-server-settings.sh   # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-server-settings.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-server-settings.sh   # a CLI built elsewhere (another worktree)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-server-settings}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
handle="owner$stamp"
member="member$stamp"
name="Settings smoke"
rename="Renamed on the phone"
for d in owner member; do "$cli" --data-dir "$work/$d" instance add "$instance" >/dev/null; done
"$cli" --data-dir "$work/owner" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$work/owner" server-create "$name" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$work/owner" channel-create "$server" general >/dev/null
invite=$("$cli" --data-dir "$work/owner" invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
"$cli" --data-dir "$work/member" account create --handle "$member" --password "$password" --invite "$invite" >/dev/null
echo "seeded $handle owning \"$name\" ($server), and $member in it"

# The server as the instance has it, read beside the phone as the owner.
field() { python3 -c "import sys, json; d = json.load(sys.stdin); d = d.get('server', d); print($1)"; }
token=$(curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
  python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])")
server_field() { curl -s -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/servers/$server" | field "$1"; }
[ "$(server_field 'd.get("retentionDays", 0)')" = "0" ] || { echo "FAIL: a new server does not keep its messages for ever"; exit 1; }
[ "$(server_field 'd.get("showDeleted", False)')" = "False" ] || { echo "FAIL: a new server shows its deleted messages"; exit 1; }

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
# A new phone each time: the app and the stores its group keeps across installs.
fresh() {
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
}
fresh

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT

run() { # <handle> <test>
  TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$1" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
  TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_SERVER="$name" TEST_RUNNER_LUMIA_UITEST_RENAME="$rename" \
    xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
    -only-testing:"LumiaUITests/SmokeUITests/$2" > "$work/$2.log" 2>&1 &
  uitest=$!
  log="$work/$2.log"
}

# Waits for the test to announce a step and screenshots it; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    echo "FAIL: the UI test did not reach \"$1\"; see $log"
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$log" | head -10
    exit 1
  fi
  sleep 1
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
}
seen() { touch "$work/steps/$1.seen"; }
passed() {
  wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
  uitest=""
}

run "$handle" testServerSettingsAreChangedHere
reached open 240
seen open
echo "the owner is offered Server settings; its words, field and choices are labelled, Forever chosen"
reached warning 60
seen warning
echo "30 days chosen: the screen says what it will delete before Save"
reached saved 60
listed=$("$cli" --data-dir "$work/owner" servers | grep "$server" || true)
case "$listed" in
  *"  $rename") ;;
  *) echo "FAIL: the instance lists the server as \"$listed\", not \"$rename\""; exit 1 ;;
esac
retention=$(server_field 'd.get("retentionDays", 0)')
[ "$retention" = "30" ] || { echo "FAIL: the instance keeps the server's messages for $retention days, not 30"; exit 1; }
echo "the instance has the server as \"$rename\" (trimmed), keeping its messages 30 days"

# A picture from elsewhere, solid red, which the instance squares as it keeps it.
python3 - "$work/red.png" <<'PY'
import struct, sys, zlib
w = h = 256
raw = b"".join(b"\x00" + b"\xd0\x10\x10" * w for _ in range(h))
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
PY
"$cli" --data-dir "$work/owner" server-update "$server" --icon "$work/red.png" >/dev/null
icon=$(server_field 'd.get("iconUrl", "")')
[ -n "$icon" ] || { echo "FAIL: the instance has no picture for the server after server-update --icon"; exit 1; }
echo "the server's picture is $icon"
seen saved
reached shown 60
shown=$(server_field 'd.get("showDeleted", False)')
[ "$shown" = "True" ] || { echo "FAIL: the instance does not show the server's deleted messages after the switch was turned on (showDeleted $shown)"; exit 1; }
echo "Show deleted messages turned on through the screen: showDeleted on the instance"
seen shown
reached picture 60
seen picture
echo "the picture set elsewhere reached the screen: it is labelled and Remove is offered"
reached rail 30
# The middle of the server's tile on the rail, as the test found it.
sips -s format bmp "$work/rail.png" --out "$work/rail.bmp" >/dev/null 2>&1
colour=$(python3 - "$work/rail.bmp" "$(cat "$work/steps/rail")" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
x, y, w, h = map(float, sys.argv[2].split())
start, = struct.unpack("<I", data[10:14])
width, height, _, bits = struct.unpack("<iiHH", data[18:30])
step = bits // 8
stride = (width * step + 3) // 4 * 4
px, py = int((x + w / 2) * width), int((y + h / 2) * abs(height))
row = py if height < 0 else abs(height) - 1 - py
at = start + row * stride + px * step
b, g, r = data[at], data[at + 1], data[at + 2]
print(f"{r} {g} {b}")
PY
)
rm -f "$work/rail.bmp"
seen rail
read -r red green blue <<EOF
$colour
EOF
[ "$red" -gt 150 ] && [ "$green" -lt 80 ] && [ "$blue" -lt 80 ] || { echo "FAIL: the middle of the server's tile on the rail is rgb($colour), not the picture's red; see $work/rail.png"; exit 1; }
echo "the rail draws the server's picture (rgb($colour) at the middle of its tile)"
passed

# A member who may not change the settings, on a new phone.
fresh
run "$member" testServerSettingsAreNotOfferedToAMember
reached member 240
seen member
passed
echo "a member who may not change them is not offered Server settings"
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "smoke-server-settings: ok (renamed, retention set and Show deleted messages turned on through the screen and read back from the instance, a picture set elsewhere drawn on the screen and the rail, and nothing offered to a member; screenshots under $work)"
