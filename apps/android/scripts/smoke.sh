#!/bin/sh
# Smoke-runs the Android app in an emulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, and a second account in the server
# with two of its own; installs the debug build, launches it signed in as
# the first account (the LUMIA_DEV_* extras a debug build honours, through
# the launch intent) and screenshots each screen the LUMIA_DEV_SCREEN
# switch can open. Then the channel is read as assistive technology has
# it: each account's messages in a row are one block, and the second
# account, typing through the CLI, is named under the composer until it
# falls silent (protocol 6.7), in light and dark. Then push. The emulator
# reaches the host's instance at 10.0.2.2.
#
#   scripts/smoke.sh            # expects the app built (gradle assembleDebug) and the CLI, and an emulator or device on adb
#   LUMIA_INSTANCE=http://localhost:4000 scripts/smoke.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
emulated=$(printf '%s' "$instance" | sed 's#localhost#10.0.2.2#; s#127\.0\.0\.1#10.0.2.2#')
cli="$root/apps/core/target/release/lumia-cli"
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

launch() { # <screen> <seconds>
  "$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
  "$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN "$1" >/dev/null
  sleep "$2"
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
shoot home 10
alive=$("$adb" shell pidof "$bundle" 2>/dev/null || true)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel requests devices profile new add newchannel invite signin link; do shoot "$screen" 7; done
[ -n "$alive" ] || { echo "FAIL: the app was not running after launch"; exit 1; }

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
echo "smoke: ok (launched signed in, every screen screenshotted under $work; messages in blocks and someone typing in the channel; a mention became a notification and its tap opened the channel)"
