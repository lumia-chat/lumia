#!/bin/sh
# The notification extension end to end in a simulator (architecture
# 15.5), against a dev instance: the phone is linked as a second device of
# a CLI identity (through LUMIA_DEV_LINK's files), another CLI identity
# asks for a DM, the first CLI device accepts (creating the group with the
# phone as a leaf), the other sends a message, and with the app
# terminated a push with the generic notice is delivered through simctl;
# the extension syncs and decrypts under the writer lock and the banner
# shows the message. Screenshots land in .build/smoke-push/.
#
#   scripts/smoke-push.sh            # expects the app built for the simulator and the release CLI
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli="$root/apps/core/target/release/lumia-cli"
app="$here/../Lumia-iOS/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-push}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"
bundle=chat.lumia
x="$work/x" # the identity the phone joins
y="$work/y" # the other side of the conversation

# The instance, waited for: on a busy runner a single short probe is not enough.
for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }
stamp=$(date +%s)
for d in "$x" "$y"; do "$cli" --data-dir "$d" instance add "$instance" >/dev/null; done
"$cli" --data-dir "$x" account create --handle "phone$stamp" --password "$password" >/dev/null
"$cli" --data-dir "$x" identity create --label laptop >/dev/null
"$cli" --data-dir "$x" identity link-account --password "$password" >/dev/null
server=$("$cli" --data-dir "$x" server-create "Push smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$x" channel-create "$server" general >/dev/null
xacct=$("$cli" --data-dir "$x" dm setup | grep -oE "account [0-9a-f-]{36}" | cut -d' ' -f2)
"$cli" --data-dir "$y" account create --handle "cli$stamp" --password "$password" >/dev/null
"$cli" --data-dir "$y" identity create --label desk >/dev/null
"$cli" --data-dir "$y" identity link-account --password "$password" >/dev/null
"$cli" --data-dir "$y" dm setup >/dev/null
echo "seeded phone$stamp ($xacct) and cli$stamp"

udid=$(xcrun simctl list devices available -j | python3 -c "
import json,sys
all=[d for r in json.load(sys.stdin)['devices'].values() for d in r]
devs=[d for d in all if d['name']=='$simulator'] or [d for d in all if d['name'].startswith('iPhone')]
devs.sort(key=lambda d: d['state']!='Booted')
print(devs[0]['udid'] if devs else '')")
[ -n "$udid" ] || { echo "no iPhone simulator"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
# The app group's container keeps the stores across installs; start clean.
groups="$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"
rm -rf "$groups"/*/Lumia 2>/dev/null || true

# Link the phone: the UI test launches the app signed in, answers the
# notification prompt (nothing else can), and holds it while the request
# is approved here and the code goes back; the test target was built with
# the app (xcodebuild build-for-testing, see AGENTS.md).
project="$here/../Lumia-iOS"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="phone$stamp" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_LINK=1 TEST_RUNNER_LUMIA_UITEST_HOLD=25 \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testAllowNotificationsAndHold > "$work/uitest.log" 2>&1 &
uitest=$!
request=""
for _ in $(seq 1 90); do
  f=$(ls "$groups"/*/Lumia/link-request.txt 2>/dev/null | head -1 || true)
  if [ -n "$f" ]; then request=$(cat "$f"); break; fi
  sleep 1
done
[ -n "$request" ] || {
  echo "FAIL: the app wrote no link request"
  # What the app was doing instead, for the artifact.
  xcrun simctl io "$udid" screenshot "$work/nolink.png" >/dev/null 2>&1 || true
  xcrun simctl spawn "$udid" log show --info --last 3m --style compact --predicate 'subsystem == "chat.lumia" OR process == "Lumia"' > "$work/sim.log" 2>&1 || true
  tail -20 "$work/uitest.log"
  exit 1
}
data=$(dirname "$f")
code=$("$cli" --data-dir "$x" identity approve "$request" | grep -oE "code [A-Za-z0-9]+" | cut -d' ' -f2)
echo "$code" > "$data/link-code.txt"
sleep 12
xcrun simctl io "$udid" screenshot "$work/linked.png" >/dev/null 2>&1
wait $uitest || { echo "FAIL: the UI test did not pass; see $work/uitest.log"; tail -20 "$work/uitest.log"; exit 1; }
[ "$("$cli" --data-dir "$x" identity show | grep -c "^  device ")" -ge 2 ] || { echo "FAIL: the phone is not in the roster"; "$cli" --data-dir "$x" identity show; exit 1; }
echo "linked: the phone is a device of phone$stamp's identity"

# The conversation: the other side asks, the laptop accepts (the group gets
# the phone as a leaf), the other side joins and sends.
"$cli" --data-dir "$y" dm request "$xacct" >/dev/null
req=$("$cli" --data-dir "$x" dm requests | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$x" dm respond "$req" accept >/dev/null
channel=$("$cli" --data-dir "$y" dm open "$xacct" | grep -oE "[0-9a-f-]{36}\s+personal DM" | grep -oE "^[0-9a-f-]{36}")
body="Hello from the CLI, decrypted on the phone $stamp"
"$cli" --data-dir "$y" dm send "$channel" "$body" >/dev/null
echo "sent in $channel"

# The extension's work, with the app gone. The simulator's push injection
# never launches a service extension (it adds the notification directly),
# so a debug launch stands in for it: the same call, under the same lock,
# its answer written to notification.txt; the push itself follows for the
# banner, generic here, rewritten on a device.
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
sleep 2
rm -f "$data/notification.txt"
SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="phone$stamp" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" SIMCTL_CHILD_LUMIA_DEV_SCREEN=notify SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 \
  xcrun simctl launch "$udid" "$bundle" >/dev/null
content=""
for _ in $(seq 1 40); do
  [ -f "$data/notification.txt" ] && { content=$(cat "$data/notification.txt"); break; }
  sleep 1
done
echo "notification content:"; printf '%s\n' "$content" | sed 's/^/  /'
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
store=$(ls "$data"/*/store.sqlite | grep -v /identity/ | head -1)
gen=$(sqlite3 "$store" "select value from meta where key = 'writer_generation'" 2>/dev/null || echo 0)
echo "writer generation after the extension's call: ${gen:-0}"
case "$content" in
  *"$body"*) ;;
  *) echo "FAIL: the message was not decrypted for the notice"; exit 1 ;;
esac
[ "${gen:-0}" -gt 0 ] || { echo "FAIL: no writer generation bump"; exit 1; }

# The tap: the notification as the extension rewrites it (the sender as
# the title, the channel and kind in the content) is delivered while the
# UI test holds the app in the foreground; the test taps the banner and
# expects the conversation's panel, titled with the other side's handle.
host=$(printf '%s' "$instance" | sed -E 's#^https?://##; s#/$##')
# No mutable-content here, unlike the relay's payload: the simulator cannot
# launch the service extension, and the flag makes SpringBoard wait the
# extension's whole 30 s budget before it presents anything, which was
# most of why the banner missed the test's window on the runner. The
# extension's rewrite is what the stand-in above checked.
printf '{"aps":{"alert":{"title":"cli%s","body":"%s"},"sound":"default"},"instance":"%s","channel_id":"%s","kind":"dm"}\n' "$stamp" "$body" "$host" "$channel" > "$work/push.json"
rm -f "$work/ready.txt"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="phone$stamp" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_UITEST_EXPECT="cli$stamp" TEST_RUNNER_LUMIA_UITEST_READY="$work/ready.txt" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testOpenNotification > "$work/uitest-open.log" 2>&1 &
uitest=$!
# The push waits for the test's word that the app is in the foreground: on
# the runner the test takes well over the 20 s this once slept, and a
# banner delivered to the home screen is gone before the test looks.
t0=$(date +%s)
for _ in $(seq 1 150); do [ -f "$work/ready.txt" ] && break; sleep 1; done
[ -f "$work/ready.txt" ] || { echo "FAIL: the UI test never had the app in the foreground; see $work/uitest-open.log"; tail -20 "$work/uitest-open.log"; exit 1; }
echo "the app is in the foreground after $(( $(date +%s) - t0 )) s; pushing"
sleep 3
xcrun simctl push "$udid" "$bundle" "$work/push.json" >/dev/null
sleep 2
xcrun simctl io "$udid" screenshot "$work/push.png" >/dev/null 2>&1
sleep 6
xcrun simctl io "$udid" screenshot "$work/opened.png" >/dev/null 2>&1
wait $uitest || {
  echo "FAIL: the tap did not open the conversation; see $work/uitest-open.log and $work/opened.png"
  grep -E 'error|failed' "$work/uitest-open.log" | head -5
  # The app's and the notification system's log around the push, for the artifact.
  xcrun simctl spawn "$udid" log show --info --last 4m --style compact --predicate 'subsystem == "chat.lumia" OR process == "Lumia" OR subsystem CONTAINS "usernotifications"' > "$work/sim.log" 2>&1 || true
  exit 1
}
echo "smoke-push: ok (the phone joined the group and decrypted the message for the notice; the banner is in $work/push.png, the conversation it opened in $work/opened.png)"
