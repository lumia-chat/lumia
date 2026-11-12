#!/bin/sh
# Smoke-runs the Android app in an emulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, and a second account in the server
# with two of its own; installs the debug build, launches it signed in as
# the first account (the LUMIA_DEV_* extras a debug build honours, through
# the launch intent) and screenshots each screen the LUMIA_DEV_SCREEN
# switch can open. Then Preferences: dark and compact as a launch can be
# told to look (LUMIA_DEV_APPEARANCE, LUMIA_DEV_DENSITY), and by taps from
# the account menu, the screen kept as the phone is turned, a choice kept
# across a launch and put back, the screen kept as the phone goes dark.
# Then the channel is read as assistive technology has it: each account's
# messages in a row are one block, and the second account, typing through
# the CLI, is named under the composer until it falls silent (protocol
# 6.7), in light and dark. Then push. Then signing out from the account
# menu: the phone's push subscription and its session must be gone from
# the instance, the sign-in screen must say nothing of it, a push that
# still arrives must show nothing, and signing in again must subscribe
# again. Then another account on the same phone: signed out of the first
# and into a third the script made, typed into the sign-in screen, the
# phone must show that account and nothing of the first, hold the phone's
# one session, and be sent the notices for it and none for the first.
# Last, the phone's session is revoked from elsewhere: with the app away,
# the instance must stop sending it notices before the phone has done
# anything, and the app opened finds itself signed out and says so; and
# with the app open, it must go back to sign-in and say so. The emulator
# reaches the host's instance at 10.0.2.2.
#
# Nothing bounds how long this takes but whoever runs it (no workflow
# does); it waits for what it needs and no longer where it can tell.
#
#   scripts/smoke.sh            # expects the app built (gradle assembleDebug) and the CLI, and an emulator or device on adb
#   LUMIA_INSTANCE=http://localhost:4000 scripts/smoke.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke.sh   # a CLI built elsewhere (a git worktree has none of its own)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
emulated=$(printf '%s' "$instance" | sed 's#localhost#10.0.2.2#; s#127\.0\.0\.1#10.0.2.2#')
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
apk="$here/../app/build/outputs/apk/debug/app-debug.apk"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
adb="$ANDROID_HOME/platform-tools/adb"

for _ in $(seq 1 60); do curl -s -m 2 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -f "$apk" ] || { echo "no apk at $apk"; exit 1; }
"$adb" get-state >/dev/null 2>&1 || { echo "no device on adb; start an emulator (scripts/emulator.sh)"; exit 1; }

stamp=$(date +%s)
handle="android$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Android smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" send "$channel" "Hello from the CLI" >/dev/null
"$cli" --data-dir "$data" send "$channel" "The Android app reads this through the same core." >/dev/null
# A second account, joined through an invite the phone's account mints:
# the other person in the channel. Two messages in a row from each are a
# block each; it is typing further down, and mentions the phone's account
# for the push after that.
other="$work/other"
other_handle="mention$stamp"
"$cli" --data-dir "$other" instance add "$instance" >/dev/null
"$cli" --data-dir "$other" account create --handle "$other_handle" --password "$password" >/dev/null
invite=$("$cli" --data-dir "$data" invite-create "$server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null \
  || { echo "FAIL: the second account could not join the server through the invite $invite"; exit 1; }
"$cli" --data-dir "$other" send "$channel" "Hello from someone else" >/dev/null
"$cli" --data-dir "$other" send "$channel" "Two in a row from one person are one block." >/dev/null
# A third account, for the phone to sign in as once it has signed out of
# the first: in no server of the first's, with one of its own that the
# second account joins, so there is someone to mention it there.
next="$work/next"
next_handle="next$stamp"
"$cli" --data-dir "$next" instance add "$instance" >/dev/null
"$cli" --data-dir "$next" account create --handle "$next_handle" --password "$password" >/dev/null
next_server=$("$cli" --data-dir "$next" server-create "Next smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$next" channel-create "$next_server" general >/dev/null
next_channel=$("$cli" --data-dir "$next" channels "$next_server" | grep -oE "[0-9a-f-]{36}" | head -1)
invite=$("$cli" --data-dir "$next" invite-create "$next_server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null \
  || { echo "FAIL: the second account could not join the third's server through the invite $invite"; exit 1; }
next_me=$("$cli" --data-dir "$next" whoami)
echo "seeded $handle, $other_handle and $next_handle, server $server, channel $channel"

"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null
# The phone is turned further down, and left upright and as it was found.
turning=$("$adb" shell settings get system accelerometer_rotation 2>/dev/null | tr -d '\r')
listener=""
hush() {
  [ -z "$listener" ] || { kill "$listener" 2>/dev/null || true; wait "$listener" 2>/dev/null || true; }
  listener=""
}
tidy() {
  hush
  "$adb" shell settings put system user_rotation 0 >/dev/null 2>&1 || true
  "$adb" shell settings put system accelerometer_rotation "${turning:-0}" >/dev/null 2>&1 || true
}
trap tidy EXIT

# Whose session a launch asks for; a launch keeps a session the phone already holds, whoever is named.
who=$handle
launch() { # <screen> <seconds> [more for am start: --es LUMIA_DEV_APPEARANCE dark, --es LUMIA_DEV_DENSITY compact]
  screen=$1 seconds=$2
  shift 2
  "$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
  "$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$who" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN "$screen" "$@" >/dev/null
  sleep "$seconds"
}
shot() { # <name>
  "$adb" exec-out screencap -p > "$work/$1.png"
  echo "  $1: $work/$1.png"
}
shoot() { # <screen> <seconds>
  launch "$1" "$2"
  shot "$1"
}
# The screen as assistive technology has it: every text and description.
ui() {
  "$adb" shell uiautomator dump /sdcard/lumia-ui.xml >/dev/null 2>&1 || true
  "$adb" shell cat /sdcard/lumia-ui.xml 2>/dev/null || true
}
count() { # <text> <pattern>: how many times
  printf '%s' "$1" | grep -o "$2" | wc -l | tr -d ' '
}
# Whether the screen comes to read <text> within <seconds> (twenty): a wait
# for what is waited for, as long as that takes and no longer.
await() { # <text> [seconds]
  for _ in $(seq 1 "${2:-20}"); do
    case "$(ui)" in *"$1"*) return 0 ;; esac
    sleep 1
  done
  return 1
}
# The phone upright (0) or on its side (1). Android makes the activity again for it.
turn() { # <0|1>
  "$adb" shell settings put system accelerometer_rotation 0
  "$adb" shell settings put system user_rotation "$1"
  sleep 3
}
running() { "$adb" shell pidof "$bundle" 2>/dev/null | tr -d '\r' || true; }
# One fact about the first thing on screen whose text or description is the
# one named: where its middle is, whether it is the choice made (a radio
# button that is checked), how far down the screen it starts, or whether a
# screen reader has it as a button (Compose gives what takes the tap a node
# of the Button class beside the text, inside the one that is clickable).
node() { # <text or description> <middle|chosen|top|button>
  fact "$(ui)" "$1" "$2"
}
# The same of a screen already read, for several facts about one screen.
fact() { # <screen> <text or description> <middle|chosen|top|button>
  printf '%s' "$1" | python3 -c '
import re, sys
want, fact = sys.argv[1], sys.argv[2]
nodes = re.findall(r"<node [^>]*>", sys.stdin.read())
def box(node):
    return tuple(int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
def within(a, b):
    return a[0] >= b[0] and a[1] >= b[1] and a[2] <= b[2] and a[3] <= b[3]
for node in nodes:
    if "text=\"%s\"" % want in node or "content-desc=\"%s\"" % want in node:
        x0, y0, x1, y1 = box(node)
        taps = sorted((box(n) for n in nodes if "clickable=\"true\"" in n and within(box(node), box(n))), key=lambda b: (b[2] - b[0]) * (b[3] - b[1]))
        button = bool(taps) and any("class=\"android.widget.Button\"" in n and within(box(n), taps[0]) for n in nodes)
        print({"middle": "%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2), "chosen": "yes" if "checked=\"true\"" in node else "no", "top": y0, "button": "yes" if button else "no"}[fact])
        break
' "$2" "$3"
}
tap() { # <text or description>
  at=$(node "$1" middle)
  [ -n "$at" ] || { echo "FAIL: nothing on screen reads \"$1\" to tap"; exit 1; }
  # shellcheck disable=SC2086
  "$adb" shell input tap $at
  sleep 1
}
# What the app has stored on the phone beside the saved instance.
stored() {
  "$adb" shell run-as "$bundle" cat shared_prefs/lumia.xml 2>/dev/null || true
}
# What the phone remembers of the push subscriptions it has sent.
marks() {
  "$adb" shell run-as "$bundle" cat shared_prefs/push.xml 2>/dev/null || true
}
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
# A session of the script's own beside the phone's, over HTTP, as the phone's account or the one named: its token.
sign_in() { # [handle]
  curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"${1:-$handle}\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]'
}
# The account's sessions as the instance lists them for <token>: an id and its device a line.
sessions() { # <token>
  curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" | field '"\n".join(sorted(s["sessionId"] + " " + s.get("deviceId", "") for s in d.get("sessions", [])))'
}
# The lines of the first list that the second does not have.
missing() { # <list> <list>
  printf '%s\n' "$1" | while read -r line; do
    [ -z "$line" ] || printf '%s\n' "$2" | grep -qxF "$line" || echo "$line"
  done
}
lines() { # <list>: how many
  printf '%s' "$1" | grep -c . || true
}
# The room the channel's first messages take: how far under the first the
# one sent after the others starts.
room() {
  seen=$(ui)
  first=$(fact "$seen" "Hello from the CLI" top)
  last=$(fact "$seen" "Still here?" top)
  [ -n "$first" ] && [ -n "$last" ] || { echo "FAIL: the channel does not show the messages the script sent" >&2; exit 1; }
  echo $((last - first))
}
launch home 0
await 'text="connected"' || { shot home; echo "FAIL: the app launched signed in did not connect; see $work/home.png"; exit 1; }
shot home
alive=$(running)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel requests devices profile preferences new add newchannel invite signin link; do shoot "$screen" 7; done
[ -n "$alive" ] || { echo "FAIL: the app was not running after launch"; exit 1; }

# Preferences. First how a launch can be told to look, for the eye: the
# Preferences screen dark on a light phone, and the channel compact beside
# channel.png, which is comfortable. Compact must give the same messages
# less room, and none of it may be kept: the extras are that launch's, and
# what is drawn after them is as the phone chose.
launch channel 7
roomy=$(room)
launch preferences 7 --es LUMIA_DEV_APPEARANCE dark
shot preferences-dark
[ "$(node Dark chosen)" = yes ] || { echo "FAIL: a launch told to be dark does not show Dark as the choice; see $work/preferences-dark.png"; exit 1; }
launch channel 7 --es LUMIA_DEV_DENSITY compact
shot channel-compact
tight=$(room)
[ "$tight" -gt 0 ] && [ "$tight" -lt "$roomy" ] \
  || { echo "FAIL: compact did not give the channel's messages less room ($tight against $roomy); see $work/channel-compact.png"; exit 1; }
case "$(stored)" in *appearance*|*density*) echo "FAIL: a launch's LUMIA_DEV_APPEARANCE or LUMIA_DEV_DENSITY was kept on the phone"; exit 1 ;; esac
# Then as a person gets there and chooses: the rail's own avatar opens the
# account's menu, the menu leads to Preferences, each choice is the one
# shown at the next launch, and then both are put back, so what follows is
# drawn as it was before.
launch home 7
tap "$handle, your account"
shot account
# The menu's last item is the way out, under the rest, and a button named "Sign out" to a screen reader.
seen=$(ui)
[ "$(fact "$seen" "Sign out" button)" = yes ] || { echo "FAIL: the account menu has no button named \"Sign out\"; see $work/account.png"; exit 1; }
for item in "View profile" Preferences Devices; do
  [ "$(fact "$seen" "Sign out" top)" -gt "$(fact "$seen" "$item" top)" ] || { echo "FAIL: Sign out is not under $item in the account menu; see $work/account.png"; exit 1; }
done
tap Preferences
seen=$(ui)
case "$seen" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the account menu did not lead to Preferences; see $work/account.png"; exit 1 ;; esac
[ "$(fact "$seen" System chosen)" = yes ] && [ "$(fact "$seen" Comfortable chosen)" = yes ] \
  || { echo "FAIL: a phone that has chosen nothing is not System and Comfortable"; exit 1; }
# The phone turned with the screen open, and turned back. Android makes
# the activity again for that, and it must be the same app over the same
# session: the process it was, and the screen the person was on, which
# taps led to and nothing of the launch names.
process=$(running)
for way in 1 0; do
  turn "$way"
  [ "$way" = 0 ] || shot preferences-turned
  case "$(ui)" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the phone was turned and the app left the screen it was on; see $work/preferences-turned.png"; exit 1 ;; esac
  [ -n "$process" ] && [ "$(running)" = "$process" ] || { echo "FAIL: the app did not survive the phone being turned"; exit 1; }
done
tap Dark
tap Compact
launch preferences 7
shot preferences-kept
seen=$(ui)
[ "$(fact "$seen" Dark chosen)" = yes ] && [ "$(fact "$seen" Compact chosen)" = yes ] \
  || { echo "FAIL: Dark and Compact were chosen and the next launch does not have them; see $work/preferences-kept.png"; exit 1; }
tap System
tap Comfortable
launch preferences 7
seen=$(ui)
[ "$(fact "$seen" System chosen)" = yes ] && [ "$(fact "$seen" Comfortable chosen)" = yes ] \
  || { echo "FAIL: System and Comfortable were chosen again and the next launch does not have them"; exit 1; }
# System is whichever the phone is in, as it changes: the phone goes dark
# with the screen open, the picture is dark for the eye, and the screen is
# still the one the person was on (a phone going dark at sunset does not
# send anyone back to Home).
night=$("$adb" shell cmd uimode night 2>/dev/null | grep -oE "yes|no|auto" | head -1)
"$adb" shell cmd uimode night yes >/dev/null
sleep 3
shot preferences-system-dark
seen=$(ui)
"$adb" shell cmd uimode night "${night:-no}" >/dev/null
case "$seen" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the phone went dark and the app left the screen it was on; see $work/preferences-system-dark.png"; exit 1 ;; esac
echo "  preferences: ok (dark and compact for a launch and not kept, $tight against $roomy; reached from the account menu; the screen kept as the phone was turned and turned back; a choice kept across a launch and put back; the screen kept as the phone went dark)"

# Blocks, in the channel: the second account's two messages in a row are
# under its name once, and the second says whose it is and when to a
# screen reader, which is all that says it on a phone. The phone's account
# has two blocks, its first two messages and the one sent after the
# others, so it is named twice and described once.
launch channel 7
seen=$(ui)
[ "$(count "$seen" "text=\"$other_handle\"")" = 1 ] && [ "$(count "$seen" "content-desc=\"$other_handle, ")" = 1 ] \
  || { echo "FAIL: two messages in a row from $other_handle are not one block; see $work/channel.png"; exit 1; }
[ "$(count "$seen" "text=\"$handle\"")" = 2 ] && [ "$(count "$seen" "content-desc=\"$handle, ")" = 1 ] \
  || { echo "FAIL: the messages of $handle are not the two blocks they should be; see $work/channel.png"; exit 1; }

# Who is typing (protocol 6.7): the second account says so once through
# the CLI, the line under the composer names it, and nine seconds of
# silence end it (the instance's eight, and the wait for the CLI).
"$cli" --data-dir "$other" typing "$channel" >/dev/null &
typing=""
for _ in $(seq 1 8); do
  case "$(ui)" in *"$other_handle is typing"*) typing=yes; break ;; esac
done
shot typing
wait
[ -n "$typing" ] || { echo "FAIL: $other_handle was typing and the channel did not say so; see $work/typing.png"; exit 1; }
sleep 9
case "$(ui)" in *"is typing"*) echo "FAIL: someone is still shown typing after nine seconds of silence"; exit 1 ;; esac

# The same in the dark theme, for the eye.
night=$("$adb" shell cmd uimode night 2>/dev/null | grep -oE "yes|no|auto" | head -1)
"$adb" shell cmd uimode night yes >/dev/null
launch channel 7
"$cli" --data-dir "$other" typing "$channel" >/dev/null &
sleep 3
shot typing-dark
wait
"$adb" shell cmd uimode night "${night:-no}" >/dev/null
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  blocks and typing: ok ($other_handle was named once over two messages, shown typing, and no longer after its silence)"

# Push (architecture 15.5): the second account mentions the phone's
# account while the app is away. The receivers' stand-in, a launch with
# LUMIA_DEV_SCREEN=notify, makes the same call under the same lock as a
# push would and posts the notification, its content recorded in
# notification.txt; then the notification's intent, as a tap sends it,
# opens the channel.
me=$("$cli" --data-dir "$data" whoami)
body="Hey, this is for you on the phone $stamp"
"$cli" --data-dir "$other" send "$channel" "$body" --mention "$me" >/dev/null
"$adb" shell pm grant "$bundle" android.permission.POST_NOTIFICATIONS >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN notify >/dev/null
content=""
for _ in $(seq 1 40); do
  content=$("$adb" shell run-as "$bundle" cat files/lumia/notification.txt 2>/dev/null || true)
  [ -n "$content" ] && break
  sleep 1
done
echo "notification content:"; printf '%s\n' "$content" | sed 's/^/  /'
sleep 1
"$adb" exec-out screencap -p > "$work/notify.png"
posted=$("$adb" shell dumpsys notification --noredact 2>/dev/null | grep -c "$stamp" || true)
case "$content" in
  *"$body"*) ;;
  *) echo "FAIL: the mention was not found for the notice"; exit 1 ;;
esac
[ "${posted:-0}" -gt 0 ] || { echo "FAIL: no notification was posted"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es channel_id "$channel" --es kind mention >/dev/null
sleep 8
"$adb" exec-out screencap -p > "$work/opened.png"
ui | grep -q "# general" || { echo "FAIL: the tap did not open the channel; see $work/opened.png"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  push: $work/notify.png (the notification), $work/opened.png (the channel it opened)"

# Signing out, from the account menu, as a person does it. For the script
# to see the phone's push subscription held and then gone, the phone is
# given an endpoint the script listens on (LUMIA_DEV_PUSH, a distributor's
# stand-in): the instance posts its notice there for a mention while the
# phone has no live session, and must stop once the phone has signed out.
: > "$work/pushes.log"
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -c '
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
class Push(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length") or 0))
        with open(sys.argv[2], "a") as log:
            log.write(self.path + " " + body.decode("utf-8", "replace").replace("\n", " ") + "\n")
        self.send_response(201)
        self.end_headers()
    def log_message(self, *args):
        pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), Push).serve_forever()
' "$port" "$work/pushes.log" &
listener=$!
endpoint="http://127.0.0.1:$port/phone-$stamp"
pushes() { wc -l < "$work/pushes.log" | tr -d ' '; }
# The second account mentions the phone's first account in its channel, or the account named in the channel named.
mention() { # <text> [account] [channel]
  "$cli" --data-dir "$other" send "${3:-$channel}" "$1" --mention "${2:-$me}" >/dev/null
}
subscribed() { # whether the phone remembers sending the endpoint, within twenty seconds
  for _ in $(seq 1 20); do
    case "$(marks)" in *unified_push*"$endpoint"*) return 0 ;; esac
    sleep 1
  done
  return 1
}
pushed() { # <count>: whether the instance has posted that many notices, within twenty seconds
  for _ in $(seq 1 20); do
    [ "$(pushes)" -ge "$1" ] && return 0
    sleep 1
  done
  return 1
}
# The endpoint stands in for a distributor's: the launch hands it to the
# sign-in's registration, which is what sends it (`PushSetup.standIn`).
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
subscribed || { echo "FAIL: a launch given an endpoint did not subscribe the phone with it"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
asked=$(date +%s)
mention "Signed in and away, the phone is sent a notice $stamp"
# An instance that cannot reach this machine's loopback (one that is not
# on it) posts nowhere the script can see: then only the phone's side of
# the subscription is checked, and the last line says so.
held=""
pushed 1 && held=yes
# How long the script waits before it says no notice came: three times
# what this one took to come, and three seconds. Nothing, where none can.
quiet=$(( ($(date +%s) - asked) * 3 + 3 ))
[ -n "$held" ] || quiet=0
token=$(sign_in)
before=$(sessions "$token")
# The menu in the dark theme, for the eye; the back button closes it and leaves the shell.
launch home 7 --es LUMIA_DEV_APPEARANCE dark
tap "$handle, your account"
shot account-dark
"$adb" shell input keyevent 4
sleep 1
seen=$(ui)
case "$seen" in *'text="Sign out"'*) echo "FAIL: the back button did not close the account menu"; exit 1 ;; esac
case "$seen" in *"$handle, your account"*) ;; *) echo "FAIL: the back button closed more than the account menu"; exit 1 ;; esac
launch home 0
await 'text="connected"' || { echo "FAIL: the app was not connected before signing out"; exit 1; }
tap "$handle, your account"
tap "Sign out"
out=""
for _ in $(seq 1 20); do
  seen=$(ui)
  case "$seen" in *"Welcome back"*) out=yes; break ;; esac
  sleep 1
done
shot signed-out
[ -n "$out" ] || { echo "FAIL: Sign out did not lead to the sign-in screen; see $work/signed-out.png"; exit 1; }
# The line is for a session the instance ended, not for one the person chose to end.
case "$seen" in *"You were signed out"*) echo "FAIL: the sign-in screen after signing out says the phone was signed out; see $work/signed-out.png"; exit 1 ;; esac
# On the instance: one session fewer, the phone's, and no other change.
after=$(sessions "$token")
gone=$(missing "$before" "$after")
[ "$(lines "$gone")" = 1 ] && [ -z "$(missing "$after" "$before")" ] \
  || { echo "FAIL: signing out did not end the phone's session on the instance, and only that (before: $before; after: $after)"; exit 1; }
# On the phone: the subscription it sent is forgotten; the instance and the preferences are the phone's and stay.
case "$(marks)" in *unified_push*) echo "FAIL: the phone signed out and still remembers the subscription it sent"; exit 1 ;; esac
kept=$(stored)
case "$kept" in *'"instance">'*) ;; *) echo "FAIL: signing out took the saved instance with it"; exit 1 ;; esac
case "$kept" in *'"appearance">system<'*'"density">comfortable<'*|*'"density">comfortable<'*'"appearance">system<'*) ;; *) echo "FAIL: signing out took the phone's preferences with it"; exit 1 ;; esac
# The launch after it is the sign-in screen, with nothing to say.
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "Welcome back" || { echo "FAIL: the launch after signing out is not the sign-in screen"; exit 1; }
case "$(ui)" in *"You were signed out"*) echo "FAIL: the launch after signing out says the phone was signed out"; exit 1 ;; esac
# The instance sends the signed-out phone nothing: the same mention as
# before, and no notice for it in the time the first took several times over.
sent=$(pushes)
mention "Signed out, the phone is sent nothing $stamp"
sleep "$quiet"
[ "$(pushes)" = "$sent" ] || { echo "FAIL: the instance still sent the signed-out phone a notice; see $work/pushes.log"; exit 1; }
# And a push that reaches it all the same (a subscription the instance
# could not be asked to drop) shows nothing, the generic notice neither:
# the receivers' stand-in again, signed out this time.
"$adb" shell run-as "$bundle" rm -f files/lumia/notification.txt >/dev/null 2>&1 || true
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_SCREEN notify >/dev/null
content=""
for _ in $(seq 1 20); do
  content=$("$adb" shell run-as "$bundle" cat files/lumia/notification.txt 2>/dev/null || true)
  [ -n "$content" ] && break
  sleep 1
done
sleep 2
[ "$content" = none ] || { echo "FAIL: a push for the signed-out phone found something to say: $content"; exit 1; }
posted=$("$adb" shell dumpsys notification --noredact 2>/dev/null | grep -c "NotificationRecord.*pkg=$bundle " || true)
[ "${posted:-0}" = 0 ] || { echo "FAIL: a push for the signed-out phone posted a notification"; exit 1; }
case "$(ui)" in *"Welcome back"*) ;; *) echo "FAIL: a push for the signed-out phone took it off the sign-in screen"; exit 1 ;; esac
# Signing in again subscribes again, with the endpoint the phone had sent
# before and forgot having sent, from the device it was: the identity and
# the device's keys stayed. And the instance sends it notices again.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
subscribed || { echo "FAIL: the phone signed in again and did not subscribe again"; exit 1; }
again=$(missing "$(sessions "$token")" "$after")
device=${gone#* }
[ "$(lines "$again")" = 1 ] && [ "${again#* }" = "$device" ] && [ -n "$device" ] \
  || { echo "FAIL: the phone signed in again and is not the device it was (was: $gone; is: $again)"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
[ "$(pushes)" = "$sent" ] || { echo "FAIL: a notice for the signed-out phone arrived late; see $work/pushes.log"; exit 1; }
mention "Signed in again and away, the phone is sent a notice $stamp"
if [ -n "$held" ]; then
  pushed $((sent + 1)) || { echo "FAIL: the phone signed in again and the instance sent it no notice; see $work/pushes.log"; exit 1; }
  subscription="the instance sent the phone a notice before, none while it was signed out, and one after it signed in again"
else
  subscription="the instance did not reach the script's listener at $endpoint, so the subscription's removal was checked on the phone only"
fi
echo "  signed out: $work/account-dark.png (the menu, dark), $work/signed-out.png (the sign-in screen, which said nothing of it; the session was gone from the instance; $subscription; a push that still arrived showed nothing; the next launch was sign-in; the phone's preferences and its device were kept)"

# Another account on the same phone, as a person changes it: signed out of
# the first from the menu and, without the app being closed, into the
# third, typed into the sign-in screen. Nothing of the first may be left:
# the phone shows the third account and its server and none of the
# first's, registers for push again by itself (the registration is once
# per sign-in, and the endpoint given at launch is handed over again), is
# the same device to the instance with one session, the third account's,
# and is sent the notices for that account and none for the first.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
await 'text="connected"' || { echo "FAIL: the app was not connected before signing out to change accounts"; exit 1; }
process=$(running)
tap "$handle, your account"
tap "Sign out"
await "Welcome back" || { echo "FAIL: Sign out did not lead to the sign-in screen the second time"; exit 1; }
tap "you@example.com"
"$adb" shell input text "$next_handle"
tap "Your password"
"$adb" shell input text "$password"
tap "Sign in"
await "$next_handle, your account" || { shot next-account; echo "FAIL: signing in as $next_handle did not open the app as that account; see $work/next-account.png"; exit 1; }
await 'text="connected"' || { shot next-account; echo "FAIL: signed in as $next_handle, the app did not connect; see $work/next-account.png"; exit 1; }
shot next-account
seen=$(ui)
[ "$(running)" = "$process" ] || { echo "FAIL: the app did not stay running from one account to the next"; exit 1; }
case "$seen" in *"You were signed out"*) echo "FAIL: the phone says it was signed out after signing in as another account"; exit 1 ;; esac
case "$seen" in *'content-desc="Next smoke"'*) ;; *) echo "FAIL: the phone signed in as $next_handle does not show its server; see $work/next-account.png"; exit 1 ;; esac
case "$seen" in *"Android smoke"*|*"$handle"*) echo "FAIL: the phone signed in as $next_handle still shows something of $handle; see $work/next-account.png"; exit 1 ;; esac
subscribed || { echo "FAIL: the phone signed in as another account did not subscribe again"; exit 1; }
next_token=$(sign_in "$next_handle")
under_next=$(sessions "$next_token" | grep -c " $device\$" || true)
under_first=$(sessions "$token" | grep -c " $device\$" || true)
[ "$under_next" = 1 ] && [ "$under_first" = 0 ] \
  || { echo "FAIL: the phone's device should hold one session, under $next_handle: it holds $under_next there and $under_first under $handle"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
if [ -n "$held" ]; then
  sent=$(pushes)
  mention "For the account the phone signed out of $stamp"
  sleep "$quiet"
  [ "$(pushes)" = "$sent" ] || { echo "FAIL: the phone signed in as $next_handle was sent a notice for $handle; see $work/pushes.log"; exit 1; }
  mention "For the account the phone signed in as $stamp" "$next_me" "$next_channel"
  pushed $((sent + 1)) || { echo "FAIL: the phone signed in as $next_handle was sent no notice for it; see $work/pushes.log"; exit 1; }
  notices="a mention of it brought the phone a notice and a mention of $handle none"
else
  notices="the instance did not reach the script's listener, so the notices were not checked"
fi
echo "  another account: $work/next-account.png (signed out of $handle and in as $next_handle in the app, which showed that account and its server and nothing of the first; one session for the phone's device, under $next_handle; subscribed again by itself; $notices)"

# Signed out from elsewhere while the app is away (protocol 12): the
# script, signed in beside the phone over HTTP, revokes every other session
# of the account, as "sign out everywhere" does. The instance itself
# removes the device's push subscription with its last session, so a
# mention brings no notice before the phone has done anything: the app is
# not running. Opened after that, the app holds tokens the instance
# refuses, goes to the sign-in screen and says so.
revoke_others() { # <token>: every session of the account but the token's own
  others=$(curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" | field '" ".join(s["sessionId"] for s in d.get("sessions", []) if not s.get("current"))')
  [ -n "$others" ] || { echo "FAIL: the account has no session but the script's"; exit 1; }
  for id in $others; do
    answer=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE -H "authorization: Bearer $1" "$instance/api/v1/sessions/$id")
    [ "$answer" = 204 ] || { echo "FAIL: revoking a session answered $answer"; exit 1; }
  done
}
who=$next_handle
revoke_others "$next_token"
if [ -n "$held" ]; then
  sent=$(pushes)
  mention "For a phone signed out from elsewhere $stamp" "$next_me" "$next_channel"
  sleep "$quiet"
  [ "$(pushes)" = "$sent" ] || { echo "FAIL: the instance sent a notice to a phone whose session it had revoked; see $work/pushes.log"; exit 1; }
  away="a mention then brought no notice, the app not having run"
else
  away="the instance did not reach the script's listener, so the notice was not checked"
fi
hush
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "You were signed out of this phone" || { shot revoked-away; echo "FAIL: opened after its session was revoked, the app did not go to sign-in and say so; see $work/revoked-away.png"; exit 1; }
shot revoked-away
case "$(marks)" in *unified_push*) echo "FAIL: the session had ended and the phone still remembers the subscription it sent"; exit 1 ;; esac
echo "  revoked while away: $work/revoked-away.png ($away; opened, the app went to sign-in and said why)"

# The same with the app open: the gateway is told and its loop ends, the
# app asks for the snapshot, is refused, and goes back to the sign-in
# screen, which says what happened. The tokens went with the session, so
# the launch after that is the sign-in screen and says nothing. Last,
# because the phone's account loses every session it had. The phone signs
# in again first, and subscribes; a session ended for it cannot ask the
# instance to drop that, and what the phone remembered sending is
# forgotten all the same, so the next sign-in sends it again.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
# The phone's store has held two accounts by now, and the second's account
# log has moved since it signed in (the mentions): the gateway must still
# take this account's events. The line under Home says how it stands.
await 'text="connected"' || {
  shot not-connected
  line=$(ui | grep -oE 'text="(connecting|reconnecting|disconnected|closed|not connected)[^"]*"' | head -1)
  echo "FAIL: signed in again as $next_handle, the second account this phone has held, the app did not connect (${line:-nothing said}); see $work/not-connected.png"
  exit 1
}
subscribed || { echo "FAIL: the phone signed in again does not remember the subscription it sent"; exit 1; }
revoke_others "$(sign_in "$next_handle")"
await "You were signed out of this phone" || { shot revoked; echo "FAIL: the session was revoked and the app did not go back to sign-in and say so; see $work/revoked.png"; exit 1; }
shot revoked
# What Preferences chose is the phone's and not the session's: it is still there.
kept=$(stored)
case "$kept" in *'"appearance">system<'*) ;; *) echo "FAIL: the session's end took the phone's appearance with it"; exit 1 ;; esac
case "$kept" in *'"density">comfortable<'*) ;; *) echo "FAIL: the session's end took the phone's message density with it"; exit 1 ;; esac
case "$(marks)" in *unified_push*) echo "FAIL: the session ended and the phone still remembers the subscription it sent"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "Welcome back" || { echo "FAIL: the launch after a revoked session is not the sign-in screen"; exit 1; }
case "$(ui)" in *"You were signed out"*) echo "FAIL: the launch after a revoked session said so a second time"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  revoked: $work/revoked.png (sent back to sign-in, which said why; the next launch did not; the phone's preferences were kept and the subscription it had sent forgotten)"
echo "smoke: ok (launched signed in, every screen screenshotted under $work; preferences reached from the account menu, kept across a launch and as the phone was turned, dark and compact drawn; messages in blocks and someone typing in the channel; a mention became a notification and its tap opened the channel; Sign out in the account menu ended the session and the push subscription and said nothing; another account signed in on the same phone was the only one it showed and was sent notices for; a session revoked from elsewhere took the subscription with it, and the app went back to sign-in and said so, opened afterwards or open at the time)"
