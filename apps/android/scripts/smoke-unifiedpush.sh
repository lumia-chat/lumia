#!/bin/sh
# A real UnifiedPush distributor against the emulator (architecture 15.5,
# plan step 8): ntfy as the distributor and ntfy.sh as its server, so the
# instance's push reaches the app through a push service nobody here runs.
# scripts/smoke.sh stands a push in with a launch that makes the receiver's
# call; this one sends the push. It needs the internet (ntfy.sh), so it is
# run by hand, not by CI.
#
# Needs:
#   - an instance the emulator reaches under the name the instance gives
#     itself. The receiver syncs only when the notice names its saved
#     instance, and the emulator reaches the host at 10.0.2.2, so a dev
#     instance started as
#       DB_URL=postgres://postgres:postgres@localhost/lumia_up PORT=4003 \
#         LUMIA_HOST=10.0.2.2 LUMIA_CODE_RELOADER=false mix phx.server
#     (after `mix ecto.create` and `mix ecto.migrate` with that DB_URL);
#   - the app built (gradle assembleDebug), the release CLI, an emulator
#     on adb (scripts/emulator.sh);
#   - the ntfy app's APK: LUMIA_NTFY_APK, else the F-Droid build of the
#     release below is downloaded into .build/.
#
#   LUMIA_INSTANCE=http://localhost:4003 scripts/smoke-unifiedpush.sh
#
# What it checks, in order: the app registers with the distributor and
# the endpoint reaches the instance; with the app's process gone and its
# session lapsed, a mention from a second account becomes a push the
# instance posts to the endpoint, ntfy hands to the app, and the app turns
# into a notification carrying the message; and the connector opened the
# notice as Web Push ciphertext rather than plain text.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4003}
emulated=$(printf '%s' "$instance" | sed 's#localhost#10.0.2.2#; s#127\.0\.0\.1#10.0.2.2#')
cli="$root/apps/core/target/release/lumia-cli"
apk="$here/../app/build/outputs/apk/debug/app-debug.apk"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-unifiedpush}
ntfy_apk=${LUMIA_NTFY_APK:-$here/../.build/ntfy.apk}
ntfy_url="https://github.com/binwiederhier/ntfy-android/releases/download/v1.25.2/ntfy-1.25.2-fdroid-release.apk"
password="correct-horse-battery-staple"
bundle=chat.lumia
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
adb="$ANDROID_HOME/platform-tools/adb"
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"

curl -s -m 5 "$instance/api/v1/instance" > "$work/instance.json" || { echo "no instance at $instance"; exit 1; }
host=$(sed -n 's/.*"host":"\([^"]*\)".*/\1/p' "$work/instance.json")
emulated_host=$(printf '%s' "$emulated" | sed 's#^[a-z]*://##; s#[:/].*##')
[ "$host" = "$emulated_host" ] || {
  echo "the instance calls itself '$host' but the emulator reaches it as '$emulated_host'; the receiver would ignore the notice (start the instance with LUMIA_HOST=$emulated_host)"
  exit 1
}
[ -f "$apk" ] || { echo "no apk at $apk"; exit 1; }
"$adb" get-state >/dev/null 2>&1 || { echo "no device on adb; start an emulator (scripts/emulator.sh)"; exit 1; }

# The distributor. A freshly installed app is stopped until opened once,
# and a stopped app gets no broadcasts, the connector's among them; its
# subscriber service is a foreground service, which needs the
# notification permission it would otherwise ask for in a dialog.
[ -f "$ntfy_apk" ] || { mkdir -p "$(dirname "$ntfy_apk")"; curl -sL -o "$ntfy_apk" "$ntfy_url"; }
"$adb" install -r "$ntfy_apk" >/dev/null
"$adb" shell pm grant io.heckel.ntfy android.permission.POST_NOTIFICATIONS >/dev/null 2>&1 || true
"$adb" shell am force-stop io.heckel.ntfy >/dev/null 2>&1 || true
# The log from here holds ntfy's connection and, later, the endpoint it
# hands the app; ntfy says "connected" once, when it gets there.
"$adb" logcat -c
"$adb" shell am start -W -n io.heckel.ntfy/.ui.MainActivity >/dev/null
sleep 3
"$adb" shell input keyevent KEYCODE_HOME

stamp=$(date +%s)
handle="up$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "UnifiedPush smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
me=$("$cli" --data-dir "$data" whoami | grep -oE "[0-9a-f-]{36}" | head -1)
other="$work/other"
"$cli" --data-dir "$other" instance add "$instance" >/dev/null
"$cli" --data-dir "$other" account create --handle "mention$stamp" --password "$password" >/dev/null
invite=$("$cli" --data-dir "$data" invite-create "$server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null
echo "seeded $handle ($me) and mention$stamp in $server"

# A fresh install: the saved session of an earlier run would otherwise
# win over the handle the extras name.
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null
"$adb" shell pm grant "$bundle" android.permission.POST_NOTIFICATIONS >/dev/null 2>&1 || true
# No LUMIA_DEV_SCREEN: the shell registers for push only on a plain launch.
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" >/dev/null
endpoint=""
for _ in $(seq 1 30); do
  endpoint=$("$adb" logcat -d 2>/dev/null | sed -n 's/.*NtfyUpDistributor: Sending NEW_ENDPOINT to chat.lumia.*): \(https[^ ]*\).*/\1/p' | tail -1)
  [ -n "$endpoint" ] && break
  sleep 2
done
[ -n "$endpoint" ] || { echo "FAIL: the distributor gave the app no endpoint"; "$adb" logcat -d | grep -i "unifiedpush\|ntfy" | tail -20; exit 1; }
echo "endpoint: $endpoint"
# ntfy delivers over a connection its service holds open to ntfy.sh.
connected=""
for _ in $(seq 1 30); do
  "$adb" logcat -d 2>/dev/null | grep -q "NtfyRepository: Connection details updated for https://ntfy.sh: state=CONNECTED" && connected=yes && break
  sleep 2
done
[ -n "$connected" ] || { echo "FAIL: ntfy never connected to ntfy.sh"; "$adb" logcat -d | grep -i "ntfy" | tail -20; exit 1; }
"$adb" exec-out screencap -p > "$work/signed-in.png"

# Away: the process gone as the OS would take it (not force-stopped, which
# would stop its receivers too), and the gateway session lapsed, which the
# instance's online check needs before it pushes.
"$adb" shell input keyevent KEYCODE_HOME
sleep 2
"$adb" shell am kill "$bundle"
echo "the app is away; waiting 120 s for its session to lapse (the gateway's idle timeout is 75 s)"
sleep 120
"$adb" logcat -c
body="Hello through ntfy $stamp"
"$cli" --data-dir "$other" send "$channel" "$body" --mention "$me" >/dev/null
echo "sent the mention at $(date +%T)"
found=""
for _ in $(seq 1 30); do
  "$adb" shell dumpsys notification --noredact 2>/dev/null | grep -A 40 "pkg=$bundle" > "$work/notification.txt" || true
  grep -q "$body" "$work/notification.txt" && found=yes && break
  sleep 2
done
"$adb" exec-out screencap -p > "$work/notification.png"
"$adb" logcat -d 2>/dev/null | grep -i "ntfy\|unifiedpush" | grep -v "wellbeing" > "$work/push.log" || true
[ -n "$found" ] || { echo "FAIL: no notification with the message; see $work/notification.txt, $work/push.log and $work/notification.png"; exit 1; }
echo "notification:"
sed -n 's/.*android.title=String (\(.*\))/  \1/p; s/.*android.text=String (\(.*\))/  \1/p' "$work/notification.txt" | head -2
if grep -q "trying with plain text" "$work/push.log"; then
  echo "FAIL: the connector got plain text; the instance did not encrypt the notice under the distributor's keys"; exit 1
fi
echo "smoke-unifiedpush: ok (the push went through ntfy.sh and the installed ntfy app as Web Push ciphertext; the notification is in $work/notification.png)"
