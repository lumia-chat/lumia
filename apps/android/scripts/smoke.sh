#!/bin/sh
# Smoke-runs the Android app in an emulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, installs the debug build, launches it
# signed in as that account (the LUMIA_DEV_* extras a debug build honours,
# through the launch intent) and screenshots each screen the
# LUMIA_DEV_SCREEN switch can open. The emulator reaches the host's
# instance at 10.0.2.2.
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
echo "seeded $handle, server $server, channel $channel"

"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null

shoot() { # <screen> <seconds>
  "$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
  "$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN "$1" >/dev/null
  sleep "$2"
  "$adb" exec-out screencap -p > "$work/$1.png"
  echo "  $1: $work/$1.png"
}
shoot home 10
alive=$("$adb" shell pidof "$bundle" 2>/dev/null || true)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel requests devices new join invite signin link; do shoot "$screen" 7; done
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
[ -n "$alive" ] || { echo "FAIL: the app was not running after launch"; exit 1; }

# Push (architecture 15.5): a second account, joined through an invite
# the phone's account mints, mentions the phone's account while the app is
# away. The receivers' stand-in, a launch with LUMIA_DEV_SCREEN=notify,
# makes the same call under the same lock as a push would and posts the
# notification, its content recorded in notification.txt; then the
# notification's intent, as a tap sends it, opens the channel.
other="$work/other"
me=$("$cli" --data-dir "$data" whoami)
"$cli" --data-dir "$other" instance add "$instance" >/dev/null
"$cli" --data-dir "$other" account create --handle "mention$stamp" --password "$password" >/dev/null
invite=$("$cli" --data-dir "$data" invite-create "$server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null \
  || { echo "FAIL: the second account could not join the server through the invite $invite"; exit 1; }
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
"$adb" shell uiautomator dump /sdcard/lumia-ui.xml >/dev/null 2>&1 || true
"$adb" shell cat /sdcard/lumia-ui.xml 2>/dev/null | grep -q "# general" || { echo "FAIL: the tap did not open the channel; see $work/opened.png"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  push: $work/notify.png (the notification), $work/opened.png (the channel it opened)"
echo "smoke: ok (launched signed in, every screen screenshotted under $work; a mention became a notification and its tap opened the channel)"
