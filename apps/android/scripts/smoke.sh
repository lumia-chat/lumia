#!/bin/sh
# Smoke-runs the Android app in an emulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, and a second account in the server
# with two of its own; installs the debug build, launches it signed in as
# the first account (the LUMIA_DEV_* extras a debug build honours, through
# the launch intent) and screenshots each screen the LUMIA_DEV_SCREEN
# switch can open. Then Preferences: dark and compact as a launch can be
# told to look (LUMIA_DEV_APPEARANCE, LUMIA_DEV_DENSITY), and by taps from
# the account menu, a choice kept across a launch and put back, the screen
# kept as the phone goes dark. Then the
# channel is read as assistive technology has
# it: each account's messages in a row are one block, and the second
# account, typing through the CLI, is named under the composer until it
# falls silent (protocol 6.7), in light and dark. Then push. Last, the
# phone's session is revoked from elsewhere, and the app must go back to
# sign-in and say so. The emulator reaches the host's instance at 10.0.2.2.
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
echo "seeded $handle and $other_handle, server $server, channel $channel"

"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null

launch() { # <screen> <seconds> [more for am start: --es LUMIA_DEV_APPEARANCE dark, --es LUMIA_DEV_DENSITY compact]
  screen=$1 seconds=$2
  shift 2
  "$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
  "$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN "$screen" "$@" >/dev/null
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
# One fact about the first thing on screen whose text or description is the
# one named: where its middle is, whether it is the choice made (a radio
# button that is checked), or how far down the screen it starts.
node() { # <text or description> <middle|chosen|top>
  ui | python3 -c '
import re, sys
want, fact = sys.argv[1], sys.argv[2]
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "text=\"%s\"" % want in node or "content-desc=\"%s\"" % want in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print({"middle": "%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2), "chosen": "yes" if "checked=\"true\"" in node else "no", "top": y0}[fact])
        break
' "$1" "$2"
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
# The room the channel's first messages take: how far under the first the
# one sent after the others starts.
room() {
  first=$(node "Hello from the CLI" top)
  last=$(node "Still here?" top)
  [ -n "$first" ] && [ -n "$last" ] || { echo "FAIL: the channel does not show the messages the script sent" >&2; exit 1; }
  echo $((last - first))
}
shoot home 10
alive=$("$adb" shell pidof "$bundle" 2>/dev/null || true)
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
tap Preferences
case "$(ui)" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the account menu did not lead to Preferences; see $work/account.png"; exit 1 ;; esac
[ "$(node System chosen)" = yes ] && [ "$(node Comfortable chosen)" = yes ] \
  || { echo "FAIL: a phone that has chosen nothing is not System and Comfortable"; exit 1; }
tap Dark
tap Compact
launch preferences 7
shot preferences-kept
[ "$(node Dark chosen)" = yes ] && [ "$(node Compact chosen)" = yes ] \
  || { echo "FAIL: Dark and Compact were chosen and the next launch does not have them; see $work/preferences-kept.png"; exit 1; }
tap System
tap Comfortable
launch preferences 7
[ "$(node System chosen)" = yes ] && [ "$(node Comfortable chosen)" = yes ] \
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
echo "  preferences: ok (dark and compact for a launch and not kept, $tight against $roomy; reached from the account menu; a choice kept across a launch and put back; the screen kept as the phone went dark)"

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

# A session revoked from elsewhere (protocol 12), as "sign out everywhere"
# or a reused refresh token does it: the gateway is told and its loop ends,
# the app asks for the snapshot, is refused, and goes back to the sign-in
# screen, which says what happened. The tokens went with the session, so
# the launch after that is the sign-in screen and says nothing. Last,
# because the phone's account loses every session it had: the script signs
# in beside them over HTTP and revokes each of the others.
launch home 10
case "$(ui)" in *'text="connected"'*) ;; *) echo "FAIL: the app was not connected before its session was revoked"; exit 1 ;; esac
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
token=$(curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]')
others=$(curl -s -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/sessions" | field '" ".join(s["sessionId"] for s in d.get("sessions", []) if not s.get("current"))')
[ -n "$others" ] || { echo "FAIL: the account has no session but the script's"; exit 1; }
for id in $others; do
  answer=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE -H "authorization: Bearer $token" "$instance/api/v1/sessions/$id")
  [ "$answer" = 204 ] || { echo "FAIL: revoking a session answered $answer"; exit 1; }
done
said=""
for _ in $(seq 1 20); do
  case "$(ui)" in *"You were signed out of this phone"*) said=yes; break ;; esac
  sleep 1
done
shot revoked
[ -n "$said" ] || { echo "FAIL: the session was revoked and the app did not go back to sign-in and say so; see $work/revoked.png"; exit 1; }
# What Preferences chose is the phone's and not the session's: it is still there.
kept=$(stored)
case "$kept" in *'"appearance">system<'*) ;; *) echo "FAIL: the session's end took the phone's appearance with it"; exit 1 ;; esac
case "$kept" in *'"density">comfortable<'*) ;; *) echo "FAIL: the session's end took the phone's message density with it"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
sleep 6
seen=$(ui)
case "$seen" in *"You were signed out"*) echo "FAIL: the launch after a revoked session said so a second time"; exit 1 ;; esac
case "$seen" in *"Welcome back"*) ;; *) echo "FAIL: the launch after a revoked session is not the sign-in screen"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  revoked: $work/revoked.png (sent back to sign-in, which said why; the next launch did not; the phone's preferences were kept)"
echo "smoke: ok (launched signed in, every screen screenshotted under $work; preferences reached from the account menu, kept across a launch, dark and compact drawn; messages in blocks and someone typing in the channel; a mention became a notification and its tap opened the channel; a session revoked from elsewhere went back to sign-in and said so)"
