#!/bin/sh
# Sign out by hand, in a simulator against a dev instance. The UI test
# (LumiaUITests/SmokeUITests/testSignOutFromTheAccountMenu) opens the
# account menu on the rail's own avatar, light and dark, taps Sign out and
# finds the sign-in screen without the line for a session the instance
# ended; signs in as a second account by typing its password, in the same
# run of the app, and finds that account's name, server and channel and
# nothing of the first's; signs that one out; signs the first in again by
# hand; is then signed out by the instance, which this script asks over
# HTTP, and finds the sign-in screen with its line; and launches the app
# with no account named, which opens on sign-in. This script seeds the two
# accounts and, at each step the test announces, looks at the screen and
# asks the instance: each account's sessions over HTTP (the phone's device
# has one under the account it is signed in to and none under the other,
# none once it has signed out, and another one after it signs in again),
# and, when LUMIA_SMOKE_DB names the instance's database, the phone's
# `relay_apns` push subscription, which nothing but the database shows: there
# once the system has given the phone a token, the signed-in account's, gone
# at each sign-out, and gone as soon as the session is revoked from outside,
# before the phone has heard of it. A simulator that is given no token (a
# runner's) has no subscription to follow, and the script says so.
#
# It also delivers a notification to the phone while each of the first
# account's two sign-ins lasts; the test reads Notification Center and finds
# it there, and gone once the account has left, chosen or ended. And one
# while the phone is signed out with the app in the foreground, which the
# test expects neither shown nor listed. (The simulator never launches the
# notification extension, so its side of that rule is the kit's tests'.)
#
# Last, a sign-out nothing answers (testSignOutWhenTheInstanceDoesNotAnswer):
# the phone signs in through a listener of this script's that passes
# everything on to the instance and then, told to, takes connections and
# says nothing. The test taps Sign out and times the sign-in screen, which
# the core's limit on every request brings in well under a minute; the
# instance, never told, still has the session, which this script then ends.
#
# Screenshots land in .build/smoke-signout/. It is not in the macOS
# workflow yet.
#
#   scripts/smoke-signout.sh     # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-signout.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-signout.sh   # a CLI built elsewhere (another worktree)
#   LUMIA_SMOKE_DB=postgres://postgres:postgres@localhost/lumia_dev scripts/smoke-signout.sh   # the push subscription too (needs psql)
#   LUMIA_UITEST_DUMP=1 scripts/smoke-signout.sh   # what the pulled-down Notification Center holds, into uitest.log
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
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }
[ -z "$db" ] || psql "$db" -Atc "select 1" >/dev/null 2>&1 || { echo "no database at LUMIA_SMOKE_DB"; exit 1; }

# Two accounts, each with a server and a channel the other has no part in.
stamp=$(date +%s)
handle="out$stamp"
other="next$stamp"
server_name="Sign-out smoke"
channel_name="first-only"
other_server_name="Second account"
other_channel_name="second-only"
seed() { # <data dir> <handle> <server> <channel>
  "$cli" --data-dir "$1" instance add "$instance" >/dev/null
  "$cli" --data-dir "$1" account create --handle "$2" --password "$password" >/dev/null
  made=$("$cli" --data-dir "$1" server-create "$3" | grep -oE "[0-9a-f-]{36}" | head -1)
  "$cli" --data-dir "$1" channel-create "$made" "$4" >/dev/null
}
seed "$work/cli" "$handle" "$server_name" "$channel_name"
seed "$work/cli-other" "$other" "$other_server_name" "$other_channel_name"
echo "seeded $handle with \"$server_name\" and $other with \"$other_server_name\""

# The instance as this script asks it: signed in beside the phone, once as
# each account, and an account's other sessions as "<session> <device>" lines.
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
host=$(curl -s -H "accept: application/json" "$instance/api/v1/instance" | field 'd["host"]')
beside() { # <handle>
  curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]'
}
token=$(beside "$handle")
other_token=$(beside "$other")
sessions() { # <token>
  curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" |
    field '"\n".join(s["sessionId"] + " " + s.get("deviceId", "") for s in d.get("sessions", []) if not s.get("current"))'
}
# The sessions an account has now that are not among those given: the phone's own.
fresh() { # <token> <known session ids>
  sessions "$1" | while read -r id device; do
    case " $2 " in *" $id "*) ;; *) [ -z "$id" ] || echo "$id $device" ;; esac
  done
}
has() { sessions "$1" | grep -q "^$2 "; } # <token> <session>
# The sessions an account has on a device.
on_device() { sessions "$1" | awk -v device="$2" '$2 == device { print $1 }'; } # <token> <device>
count() { printf '%s\n' "$1" | grep -c . || true; }
# Ends a session from outside, as the account's Devices page does on another device.
revoke() { # <token> <session>
  answer=$(curl -s -o /dev/null -w '%{http_code}' -X DELETE -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions/$2")
  [ "$answer" = 204 ] || fail "revoking session $2 answered $answer"
}
# How many `relay_apns` subscriptions the instance holds for a device, and whose they are.
subscriptions() { psql "$db" -Atc "select count(*) from push_subscriptions where kind = 'relay_apns' and device_id = '$1'"; }
subscribers() { psql "$db" -Atc "select a.handle from push_subscriptions s join accounts a on a.id = s.account_id where s.kind = 'relay_apns' and s.device_id = '$1' order by 1" | tr '\n' ' ' | sed 's/ $//'; }
before=$(sessions "$token" | cut -d' ' -f1 | tr '\n' ' ')
other_before=$(sessions "$other_token" | cut -d' ' -f1 | tr '\n' ' ')

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
listener=""
# shellcheck disable=SC2329
finish() {
  [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true
  [ -z "$listener" ] || kill "$listener" 2>/dev/null || true
}
trap finish EXIT

# What the two notifications say; the test looks for the words.
notice="A notice for $handle"
last_notice="One more for $handle"
stray_notice="A notice for nobody $stamp"
# Delivers a notification to the phone as a push would arrive, generic, as
# the instance sends it: the simulator adds it without APNs.
notify() { # <words>
  printf '{"aps":{"alert":{"title":"Lumia","body":"%s"},"sound":"default"},"instance":"%s"}\n' "$1" "$host" > "$work/push.json"
  xcrun simctl push "$udid" "$bundle" "$work/push.json" >/dev/null
}

TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_HOST="$host" \
TEST_RUNNER_LUMIA_UITEST_SERVER="$server_name" TEST_RUNNER_LUMIA_UITEST_CHANNEL="$channel_name" \
TEST_RUNNER_LUMIA_UITEST_OTHER="$other" TEST_RUNNER_LUMIA_UITEST_OTHER_SERVER="$other_server_name" TEST_RUNNER_LUMIA_UITEST_OTHER_CHANNEL="$other_channel_name" \
TEST_RUNNER_LUMIA_UITEST_NOTICE="$notice" TEST_RUNNER_LUMIA_UITEST_LAST_NOTICE="$last_notice" TEST_RUNNER_LUMIA_UITEST_STRAY_NOTICE="$stray_notice" TEST_RUNNER_LUMIA_UITEST_DUMP="${LUMIA_UITEST_DUMP:-}" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testSignOutFromTheAccountMenu > "$work/uitest.log" 2>&1 &
uitest=$!
log="$work/uitest.log"

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
    grep -E "error|failed|XCTAssert" "$log" | head -10
    fail "the UI test did not reach \"$1\"; see $log"
  fi
  sleep 1 # the screen's own fade
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
}
# Tells the test this script has looked, so it goes on.
seen() { : > "$work/steps/$1.seen"; }
# The test has run to its end, and passed.
passed() {
  wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
  uitest=""
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
  [ "$is" = "$2" ] || fail "$3 is $is, not $2; see $work/$1.png"
}
# Waits for the phone's subscription to be there, as long as a token takes
# to come, and to be the one account's.
subscribed() { # <device> <handle> <seconds>
  for _ in $(seq 1 "$3"); do
    [ "$(subscribers "$1")" = "$2" ] && return 0
    sleep 1
  done
  return 1
}

# Signed in: the instance has a session it did not have, and it is the phone's.
reached in 240
first=$(fresh "$token" "$before")
[ "$(count "$first")" = 1 ] || fail "the phone signed in and the account has not one new session but: $first"
device=${first#* }
first=${first%% *}
echo "signed in: the instance has the phone's session $first, of device $device"
held=""
if [ -n "$db" ]; then
  if subscribed "$device" "$handle" 40; then
    held=yes
    echo "the instance holds the phone's relay_apns subscription, as $handle's"
  else
    echo "note: the system gave this simulator no token, so the instance holds no subscription to see removed"
  fi
else
  echo "note: LUMIA_SMOKE_DB names no database, so the phone's push subscription is not looked at"
fi
notify "$notice"
seen in

reached menu-light 120
expect menu-light light "the account menu, light"
echo "a notification delivered to the signed-in phone is in Notification Center"
seen menu-light
reached menu-dark 120
expect menu-dark dark "the account menu, dark"
echo "the account menu ends with Sign out under its rule, a button by that name, light and dark"
seen menu-dark

# Signed out: the sign-in screen, and on the instance neither the session nor the subscription.
reached out 120
has "$token" "$first" && fail "the phone signed out and the instance still has its session $first"
[ -z "$held" ] || [ "$(subscriptions "$device")" = 0 ] || fail "the phone signed out and the instance still holds its push subscription"
echo "signed out: the sign-in screen says nothing of an ended session, Notification Center has nothing of the account, and the instance has no session $first${held:+ and no subscription for the device}"
# A push that still reaches the signed-out phone, the app in the foreground
# on sign-in: the test expects it neither shown nor listed.
notify "$stray_notice"
seen out

# The second account, by hand, in the same run of the app: the phone's
# device has one session, under that account, and the subscription is its.
reached other 180
theirs=$(fresh "$other_token" "$other_before")
[ "$(count "$theirs")" = 1 ] || fail "the phone signed in as $other and that account has not one new session but: $theirs"
[ "${theirs#* }" = "$device" ] || fail "the session of $other is of device ${theirs#* }, not the phone's $device"
theirs=${theirs%% *}
[ -z "$(on_device "$token" "$device")" ] || fail "the phone is signed in as $other and $handle still has a session of its device: $(on_device "$token" "$device")"
[ -z "$(fresh "$token" "$before")" ] || fail "the phone is signed in as $other and $handle has a new session: $(fresh "$token" "$before")"
if [ -n "$held" ]; then
  subscribed "$device" "$other" 40 || fail "the phone signed in as $other and its subscription is not that account's but: $(subscribers "$device")"
fi
echo "a push to the signed-out phone, the app in the foreground, was neither shown nor listed"
echo "signed in as $other by typing the password: the phone shows that account and nothing of $handle; its device has one session, $theirs, under $other and none under $handle${held:+, and its subscription is that account now}"
seen other

reached out-other 120
has "$other_token" "$theirs" && fail "$other signed out and the instance still has the session $theirs"
[ -z "$held" ] || [ "$(subscriptions "$device")" = 0 ] || fail "$other signed out and the instance still holds the phone's push subscription"
echo "the second account signed out: the instance has no session $theirs${held:+ and no subscription for the device}"
seen out-other

# The first account again, by hand: a session of its own, and subscribed again.
reached back 180
second=$(fresh "$token" "$before")
[ "$(count "$second")" = 1 ] || fail "the phone signed in again and the account has not one new session but: $second"
second=${second%% *}
[ "$second" != "$first" ] || fail "the session the phone signed out of is back"
[ -z "$(on_device "$other_token" "$device")" ] || fail "the phone is signed in as $handle and $other still has a session of its device"
if [ -n "$held" ]; then
  subscribed "$device" "$handle" 40 || fail "the phone signed in again and was not subscribed again"
fi
echo "signed in again as $handle by typing the password: the instance has the phone's session $second${held:+ and its subscription again}"
notify "$last_notice"
seen back

# Signed out from elsewhere: the session is revoked over HTTP, and the
# subscription goes with it on the instance, before the phone has done
# anything about it (it is told through its socket, and could unsubscribe
# nothing with a session that is over).
reached noticed 120
revoke "$token" "$second"
if [ -n "$held" ]; then
  left=$(subscriptions "$device")
  [ "$left" = 0 ] || fail "the phone's session was revoked from outside and the instance still holds $left push subscription of its device"
fi
seen noticed
reached ended 120
has "$token" "$second" && fail "the revoked session $second is still listed"
[ -z "$held" ] || [ "$(subscriptions "$device")" = 0 ] || fail "the phone was sent back to sign-in and the instance holds a push subscription of its device again"
echo "signed out from elsewhere: the instance dropped the device's subscription with the session${held:+ (the row was gone as the revocation was answered)}, the phone shows sign-in with its line, and Notification Center has nothing of the account"
seen ended

# The launch after that names no account, and no appearance: sign-in, as the simulator is.
reached next-launch 120
expect next-launch "$phone" "the sign-in screen at the next launch"
[ -z "$(fresh "$token" "$before")" ] || fail "the next launch signed in by itself"
echo "the next launch opens on sign-in, on $host, the simulator's $phone"
seen next-launch
passed

# A sign-out nothing answers. The listener passes every byte between the
# phone and the instance until the file it was given exists; from then on
# it passes nothing and answers nothing, on the connections it has and on
# any it is given, and closes none.
quiet="$work/quiet"
cat > "$work/listener.py" <<'PY'
import os, socket, sys, threading, urllib.parse
there = urllib.parse.urlsplit(sys.argv[1])
quiet = sys.argv[2]
door = socket.socket()
door.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
door.bind(("127.0.0.1", 0))
door.listen(64)
print(door.getsockname()[1], flush=True)
kept = []

def carry(source, sink):
    try:
        while True:
            data = source.recv(65536)
            if os.path.exists(quiet):
                threading.Event().wait()
            if not data:
                break
            sink.sendall(data)
    except OSError:
        pass
    if not os.path.exists(quiet):
        for end in (source, sink):
            try:
                end.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

while True:
    phone, _ = door.accept()
    kept.append(phone)
    if os.path.exists(quiet):
        continue
    instance = socket.create_connection((there.hostname, there.port or 80))
    kept.append(instance)
    threading.Thread(target=carry, args=(phone, instance), daemon=True).start()
    threading.Thread(target=carry, args=(instance, phone), daemon=True).start()
PY
python3 "$work/listener.py" "$instance" "$quiet" > "$work/listener.port" &
listener=$!
for _ in $(seq 1 50); do [ -s "$work/listener.port" ] && break; sleep 0.2; done
[ -s "$work/listener.port" ] || fail "the listener did not start"
through="http://127.0.0.1:$(cat "$work/listener.port")"
curl -s -m 5 "$through/api/v1/instance" >/dev/null || fail "the listener at $through does not pass requests on to $instance"

known=$(sessions "$token" | cut -d' ' -f1 | tr '\n' ' ')
log="$work/uitest-quiet.log"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$through" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testSignOutWhenTheInstanceDoesNotAnswer > "$log" 2>&1 &
uitest=$!

reached quiet-in 240
third=$(fresh "$token" "$known")
[ "$(count "$third")" = 1 ] || fail "the phone signed in through the listener and the account has not one new session but: $third"
third=${third%% *}
: > "$quiet"
curl -s -m 3 "$through/api/v1/instance" >/dev/null && fail "the listener still answers"
echo "signed in through the listener at $through (session $third), which now takes connections and answers nothing"
seen quiet-in

reached quiet-out 120
took=$(cat "$work/steps/quiet-out")
has "$token" "$third" || fail "the instance was never told and yet has no session $third"
echo "signed out with no answer: the sign-in screen came $took s after the tap, with nothing of the account on screen meanwhile; the instance, never told, still has session $third"
seen quiet-out
passed
revoke "$token" "$third"
# Reaped here, quietly: the shell would otherwise say it was terminated.
kill "$listener" 2>/dev/null || true
wait "$listener" 2>/dev/null || true
listener=""

xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "smoke-signout: ok (Sign out is the account menu's last item; it ends the phone's session on the instance${held:+, takes its push subscription back} and the account's notifications out of Notification Center, shows sign-in with nothing said, lets another account sign in on the same phone with nothing of the first${held:+ and the subscription as its own}, and the first again; a session revoked from outside takes the subscription on the instance and the notifications on the phone; the next launch opens on sign-in; and with no answer from the instance the sign-in screen came in $took s; screenshots under $work)"
