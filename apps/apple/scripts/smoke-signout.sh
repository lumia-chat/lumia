#!/bin/sh
# Sign out by hand, in a simulator against a dev instance. The UI test
# (LumiaUITests/SmokeUITests/testSignOutFromTheAccountMenu) opens the
# account menu on the rail's own avatar, light and dark, taps Sign out and
# finds the sign-in screen without the line for a session the instance
# ended; signs in again by typing the password, in the same run of the app;
# signs out a second time; and launches the app with no account named,
# which opens on sign-in. This script seeds the account and, at each step
# the test announces, looks at the screen and asks the instance: the
# account's sessions over HTTP (the phone's is there while it is signed in,
# gone once it has signed out, and another one after it signs in again),
# and, when LUMIA_SMOKE_DB names the instance's database, the phone's
# `relay_apns` push subscription, which nothing but the database shows: there
# once the system has given the phone a token, gone at sign-out, there again
# after signing in again. A simulator that is given no token (a runner's)
# has no subscription to follow, and the script says so. Screenshots land
# in .build/smoke-signout/. It is not in the macOS workflow yet.
#
#   scripts/smoke-signout.sh     # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-signout.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-signout.sh   # a CLI built elsewhere (another worktree)
#   LUMIA_SMOKE_DB=postgres://postgres:postgres@localhost/lumia_dev scripts/smoke-signout.sh   # the push subscription too (needs psql)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
db=${LUMIA_SMOKE_DB:-}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-signout}
rm -rf "$work" && mkdir -p "$work/steps"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }
[ -z "$db" ] || psql "$db" -Atc "select 1" >/dev/null 2>&1 || { echo "no database at LUMIA_SMOKE_DB"; exit 1; }

stamp=$(date +%s)
handle="out$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Sign-out smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
echo "seeded $handle, server $server"

# The instance as this script asks it: signed in beside the phone, once,
# and the account's other sessions as "<session> <device>" lines.
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
host=$(curl -s -H "accept: application/json" "$instance/api/v1/instance" | field 'd["host"]')
token=$(curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]')
sessions() {
  curl -s -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/sessions" |
    field '"\n".join(s["sessionId"] + " " + s.get("deviceId", "") for s in d.get("sessions", []) if not s.get("current"))'
}
# The sessions listed now that are not among those given: the phone's own.
fresh() { # <known session ids>
  sessions | while read -r id device; do
    case " $1 " in *" $id "*) ;; *) [ -z "$id" ] || echo "$id $device" ;; esac
  done
}
has() { sessions | grep -q "^$1 "; }
# How many `relay_apns` subscriptions the instance holds for a device.
subscriptions() { psql "$db" -Atc "select count(*) from push_subscriptions where kind = 'relay_apns' and device_id = '$1'"; }
before=$(sessions | cut -d' ' -f1 | tr '\n' ' ')

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
# A new phone: uninstalling takes the saved instance and the answer to the
# notification prompt; the app group's container keeps the stores across
# installs, so that goes too.
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
# What the launch that names no appearance is to look like.
phone=$(xcrun simctl ui "$udid" appearance 2>/dev/null || echo light)

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT

TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_HOST="$host" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testSignOutFromTheAccountMenu > "$work/uitest.log" 2>&1 &
uitest=$!

fail() { # <what went wrong>
  echo "FAIL: $1"
  xcrun simctl io "$udid" screenshot "$work/failed.png" >/dev/null 2>&1 || true
  exit 1
}
# Waits for the test to announce a step and screenshots it; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10
    fail "the UI test did not reach \"$1\"; see $work/uitest.log"
  fi
  sleep 1 # the screen's own fade
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
}
# Tells the test this script has looked, so it goes on.
seen() { : > "$work/steps/$1.seen"; }

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
  [ "$is" = "$2" ] || fail "$3 is $is, not $2; see $work/$1.png"
}
# Waits for the phone's subscription to be there, as long as a token takes to come.
subscribed() { # <device> <seconds>
  for _ in $(seq 1 "$2"); do
    [ "$(subscriptions "$1")" = 1 ] && return 0
    sleep 1
  done
  return 1
}

# Signed in: the instance has a session it did not have, and it is the phone's.
reached in 240
first=$(fresh "$before")
[ "$(printf '%s\n' "$first" | grep -c .)" = 1 ] || fail "the phone signed in and the account has not one new session but: $first"
device=${first#* }
first=${first%% *}
echo "signed in: the instance has the phone's session $first, of device $device"
held=""
if [ -n "$db" ]; then
  if subscribed "$device" 40; then
    held=yes
    echo "the instance holds the phone's relay_apns subscription"
  else
    echo "note: the system gave this simulator no token, so the instance holds no subscription to see removed"
  fi
else
  echo "note: LUMIA_SMOKE_DB names no database, so the phone's push subscription is not looked at"
fi
seen in

reached menu-light 60
expect menu-light light "the account menu, light"
seen menu-light
reached menu-dark 120
expect menu-dark dark "the account menu, dark"
echo "the account menu ends with Sign out under its rule, a button by that name, light and dark"
seen menu-dark

# Signed out: the sign-in screen, and on the instance neither the session nor the subscription.
reached out 90
has "$first" && fail "the phone signed out and the instance still has its session $first"
[ -z "$held" ] || [ "$(subscriptions "$device")" = 0 ] || fail "the phone signed out and the instance still holds its push subscription"
echo "signed out: the sign-in screen says nothing of an ended session, and the instance has no session $first${held:+ and no subscription for the device}"
seen out

# In again by hand, in the same run of the app: a session of its own, and subscribed again.
reached back 180
second=$(fresh "$before")
[ "$(printf '%s\n' "$second" | grep -c .)" = 1 ] || fail "the phone signed in again and the account has not one new session but: $second"
second=${second%% *}
[ "$second" != "$first" ] || fail "the session the phone signed out of is back"
if [ -n "$held" ]; then
  subscribed "$device" 40 || fail "the phone signed in again and was not subscribed again"
fi
echo "signed in again by typing the password: the instance has the phone's session $second${held:+ and its subscription again}"
seen back

reached out-again 90
has "$second" && fail "the phone signed out a second time and the instance still has its session $second"
[ -z "$held" ] || [ "$(subscriptions "$device")" = 0 ] || fail "the phone signed out a second time and the instance still holds its push subscription"
echo "signed out a second time: the instance has no session $second${held:+ and no subscription for the device}"
seen out-again

# The launch after that names no account, and no appearance: sign-in, as the simulator is.
reached next-launch 120
expect next-launch "$phone" "the sign-in screen at the next launch"
[ -z "$(fresh "$before")" ] || fail "the next launch signed in by itself"
echo "the next launch opens on sign-in, on $host, the simulator's $phone"
seen next-launch

wait "$uitest" || { echo "FAIL: the UI test did not pass; see $work/uitest.log"; grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10; exit 1; }
uitest=""
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "smoke-signout: ok (Sign out is the account menu's last item; it ends the phone's session on the instance${held:+, takes its push subscription back}, shows sign-in with nothing said, lets the same phone sign in again${held:+ and be subscribed again}, and the next launch opens on sign-in; screenshots under $work)"
