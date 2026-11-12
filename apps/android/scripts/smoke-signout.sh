#!/bin/sh
# Signing out where scripts/smoke.sh cannot go: with the phone turned
# under it, and against an instance that has stopped answering. On the
# emulator and by hand, as scripts/smoke-unifiedpush.sh is, not by CI.
#
# The phone reaches the instance through scripts/relay.py, a relay of the
# script's own, which counts the gateway connections the phone holds (one
# per open session) and, told to stall, accepts connections and answers
# none: a network that swallows what is sent.
#
# What it checks, in order:
#   - the phone turned on a screen reached by taps (Preferences, from the
#     account menu) and turned back: the same process, the same screen, one
#     gateway connection and the same sessions on the instance each time.
#     Android makes the activity again for a turn, and the store with its
#     session is the application's, not opened a second time;
#   - the relay stalls and Sign out is tapped; the phone is turned while the
#     sign-out waits, and turned back. The sign-in screen must come within
#     LUMIA_SIGNOUT_LIMIT seconds (30; the core gives the unsubscribe and
#     the sign-out call 8 each), saying nothing of a session ended, with the
#     subscription the phone had sent forgotten, no gateway connection left
#     open, and the instance, which heard none of it, still listing the
#     session;
#   - the relay passes again and the phone is turned on the sign-in screen:
#     the screen made again asks the instance again and is the sign-in
#     form; left for the launcher and come back to, the app is still
#     running (keeping the instance's answer in the saved state stopped it);
#   - the launch after that is the sign-in screen, and says nothing.
#
#   scripts/smoke-signout.sh    # expects the app built (gradle assembleDebug) and the CLI, and an emulator on adb
#   LUMIA_INSTANCE=http://localhost:4000 LUMIA_CLI=/path/to/lumia-cli scripts/smoke-signout.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
apk="$here/../app/build/outputs/apk/debug/app-debug.apk"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-signout}
limit=${LUMIA_SIGNOUT_LIMIT:-30}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
adb="$ANDROID_HOME/platform-tools/adb"

curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -f "$apk" ] || { echo "no apk at $apk"; exit 1; }
"$adb" get-state >/dev/null 2>&1 || { echo "no device on adb; start an emulator (scripts/emulator.sh)"; exit 1; }

stamp=$(date +%s)
handle="turn$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
"$cli" --data-dir "$data" server-create "Sign-out smoke" >/dev/null
echo "seeded $handle"

# The relay, on a port of this machine the emulator reaches at 10.0.2.2.
there=$(printf '%s' "$instance" | sed 's#^[a-z]*://##; s#/.*##')
case "$there" in *:*) ;; *) there="$there:80" ;; esac
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 "$here/relay.py" "$port" "${there%:*}" "${there##*:}" "$work" &
relay=$!
turned=$("$adb" shell settings get system accelerometer_rotation | tr -d '\r')
tidy() {
  kill "$relay" 2>/dev/null || true
  wait "$relay" 2>/dev/null || true
  "$adb" shell settings put system user_rotation 0 >/dev/null 2>&1 || true
  "$adb" shell settings put system accelerometer_rotation "${turned:-0}" >/dev/null 2>&1 || true
}
trap tidy EXIT
emulated="http://10.0.2.2:$port"

ui() {
  "$adb" shell uiautomator dump /sdcard/lumia-ui.xml >/dev/null 2>&1 || true
  "$adb" shell cat /sdcard/lumia-ui.xml 2>/dev/null || true
}
tap() { # <text or description>
  at=$(ui | python3 -c '
import re, sys
want = sys.argv[1]
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "text=\"%s\"" % want in node or "content-desc=\"%s\"" % want in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print((x0 + x1) // 2, (y0 + y1) // 2)
        break
' "$1")
  [ -n "$at" ] || { echo "FAIL: nothing on screen reads \"$1\" to tap"; exit 1; }
  # shellcheck disable=SC2086
  "$adb" shell input tap $at
  sleep 1
}
shot() { "$adb" exec-out screencap -p > "$work/$1.png"; }
turn() { # <0|1>: upright, or on its side
  "$adb" shell settings put system accelerometer_rotation 0
  "$adb" shell settings put system user_rotation "$1"
  sleep 3
}
running() { "$adb" shell pidof "$bundle" 2>/dev/null | tr -d '\r' || true; }
gateways() { cat "$work/gateways" 2>/dev/null || echo 0; }
one_gateway() { # whether the phone holds one gateway connection, within ten seconds, and still does two seconds on
  for _ in $(seq 1 10); do
    [ "$(gateways)" = 1 ] && break
    sleep 1
  done
  sleep 2
  [ "$(gateways)" = 1 ]
}
marks() { "$adb" shell run-as "$bundle" cat shared_prefs/push.xml 2>/dev/null || true; }
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
# The account's sessions as the instance itself lists them, asked past the relay.
sessions() { # <token>
  curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" | field '"\n".join(sorted(s["sessionId"] + " " + s.get("deviceId", "") for s in d.get("sessions", [])))'
}

"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null
turn 0
# Subscribed, with an endpoint nothing listens on, so the sign-out has a subscription to take back as well as a session to end.
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN home --es LUMIA_DEV_PUSH "http://127.0.0.1:9/phone-$stamp" >/dev/null
sleep 8
case "$(ui)" in *'text="connected"'*) ;; *) echo "FAIL: the app did not connect through the relay"; exit 1 ;; esac
case "$(marks)" in *unified_push*) ;; *) echo "FAIL: the launch given an endpoint did not subscribe the phone with it"; exit 1 ;; esac
process=$(running)
token=$(curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]')
held=$(sessions "$token")
one_gateway || { echo "FAIL: the phone holds $(gateways) gateway connections before anything, not one"; exit 1; }

# Turned on a screen, and back.
tap "$handle, your account"
tap Preferences
for way in 1 0; do
  turn "$way"
  shot "turned-$way"
  case "$(ui)" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the phone was turned and the app left the screen it was on; see $work/turned-$way.png"; exit 1 ;; esac
  [ "$(running)" = "$process" ] || { echo "FAIL: the app did not survive being turned"; exit 1; }
  one_gateway || { echo "FAIL: the phone turned holds $(gateways) gateway connections, not one: the session was opened again"; exit 1; }
  [ "$(sessions "$token")" = "$held" ] || { echo "FAIL: the phone was turned and the instance's sessions changed"; exit 1; }
done
"$adb" shell input keyevent 4
sleep 1
case "$(ui)" in *'text="connected"'*) ;; *) echo "FAIL: back from Preferences is not Home, connected"; exit 1 ;; esac
echo "  turned: ok (Preferences kept on its side and upright again, one gateway connection and the same sessions throughout)"

# Signed out of while the instance answers nothing, and turned while that waits.
tap "$handle, your account"
: > "$work/stall"
tap "Sign out"
began=$(date +%s)
said=""
out=""
turn 1
case "$(ui)" in *"You were signed out"*) said=yes ;; esac
turn 0
for _ in $(seq 1 60); do
  seen=$(ui)
  case "$seen" in *"You were signed out"*) said=yes ;; esac
  case "$seen" in *'text="Welcome'*) out=yes; break ;; esac
  [ $(($(date +%s) - began)) -lt 90 ] || break
  sleep 1
done
took=$(($(date +%s) - began))
shot signed-out
[ -n "$out" ] || { echo "FAIL: Sign out against an instance that answers nothing had not reached the sign-in screen after $took seconds; see $work/signed-out.png"; exit 1; }
[ "$took" -le "$limit" ] || { echo "FAIL: Sign out against an instance that answers nothing took $took seconds, over $limit"; exit 1; }
[ -z "$said" ] || { echo "FAIL: a sign-out the person chose, with the phone turned under it, said the phone was signed out; see $work/signed-out.png"; exit 1; }
[ "$(running)" = "$process" ] || { echo "FAIL: the app did not survive being turned while signing out"; exit 1; }
case "$(marks)" in *unified_push*) echo "FAIL: the phone signed out and still remembers the subscription it sent"; exit 1 ;; esac
# The stall was one: the instance heard nothing of it.
[ "$(sessions "$token")" = "$held" ] || { echo "FAIL: the relay was stalled and the instance's sessions changed all the same"; exit 1; }
# A signed-out phone holds no gateway connection, even one the instance
# never answered: stopping the gateway ends its loop (the core's `stop`),
# which closes the connection, within a few seconds of the sign-in screen.
open=""
for _ in $(seq 1 5); do
  [ "$(gateways)" = 0 ] && { open=0; break; }
  sleep 1
done
[ "$open" = 0 ] || { echo "FAIL: the signed-out phone still holds $(gateways) gateway connection(s): the loop was not stopped"; exit 1; }
rm -f "$work/stall"
echo "  signed out with no answer: $took seconds to the sign-in screen (limit $limit), turned on the way, nothing said of a session ended, no gateway connection left open"

# The sign-in screen turned: made again, it asks the instance again, which answers now.
for way in 1 0; do
  turn "$way"
  sleep 2
  seen=$(ui)
  case "$seen" in *"You were signed out"*) echo "FAIL: the sign-in screen turned says the phone was signed out"; exit 1 ;; esac
  case "$seen" in *"Welcome back"*) ;; *) shot "sign-in-turned-$way"; echo "FAIL: the sign-in screen turned is not the sign-in form; see $work/sign-in-turned-$way.png"; exit 1 ;; esac
done
# And left for the launcher, and come back to.
"$adb" shell input keyevent 3
sleep 3
[ "$(running)" = "$process" ] || { echo "FAIL: the app stopped when it was left on the sign-in screen"; exit 1; }
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
sleep 2
case "$(ui)" in *"Welcome back"*) ;; *) echo "FAIL: come back to, the app is not on the sign-in form"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
sleep 6
seen=$(ui)
case "$seen" in *"You were signed out"*) echo "FAIL: the launch after signing out says the phone was signed out"; exit 1 ;; esac
case "$seen" in *"Welcome back"*) ;; *) echo "FAIL: the launch after signing out is not the sign-in screen"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "smoke-signout: ok (turned on a screen with one session throughout; signed out in $took seconds against an instance that answered nothing, turned while it waited, nothing said and no gateway connection left; the sign-in screen turned and left and come back to; the next launch was sign-in)"
