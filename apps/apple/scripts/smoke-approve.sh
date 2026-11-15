#!/bin/sh
# Approving a device from the phone by hand (identity spec 4.2), against an
# instance built from this tree. An account with a server and no identity,
# signed in on a new phone with its password, so private messages are set
# up silently and the phone holds the identity; the release CLI is the new
# device and prints its link request (labelled "MacBook Pro").
# testApprovingADeviceFromThisPhone opens Approve a device from Devices,
# expects text that is not a request to be said not to be one, pastes the
# CLI's request through the system's Paste, expects "A device named
# “MacBook Pro”" and the warning, approves (the system's unlock skipped,
# LUMIA_DEV_NO_UNLOCK=1) and leaves the code it shows in a file; Done lists
# the device on Devices. The script then claims the code with the CLI and
# expects the CLI to be a device of the identity the account is bound to,
# with the phone and itself, added by a device, in its roster. Each step announced is
# screenshotted into .build/smoke-approve/. The phone's unlock is not driven.
#
#   LUMIA_INSTANCE=http://localhost:4012 LUMIA_CLI=/path/to/lumia-cli scripts/smoke-approve.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-approve}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"
bundle=chat.lumia
label="MacBook Pro"

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

handle="approve$(date +%s)"
owner="$work/cli-owner"
"$cli" --data-dir "$owner" instance add "$instance" >/dev/null
"$cli" --data-dir "$owner" account create --handle "$handle" --password "$password" >/dev/null
"$cli" --data-dir "$owner" server-create "Approve smoke" >/dev/null
# The new device: an instance known, nothing signed in, its request printed.
new="$work/cli-new"
"$cli" --data-dir "$new" instance add "$instance" >/dev/null
request=$("$cli" --data-dir "$new" identity link-request --label "$label")
[ -n "$request" ] || { echo "FAIL: the CLI printed no request"; exit 1; }
echo "seeded $handle and a request from the CLI"

# The identity the account is bound to, as the instance says it; empty for none.
bound() {
  token=$(curl -s -f -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])")
  curl -s -f -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/account/snapshot" |
    python3 -c "import sys, json; print((json.load(sys.stdin).get('account') or {}).get('identityId') or '')"
}

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

# A new phone: no identity held from an earlier run.
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true

steps="$work/steps"
mkdir -p "$steps"
(
  while :; do
    for f in "$steps"/*; do
      case "$f" in *.seen|*.png|*'*') continue ;; esac
      [ -f "$f.seen" ] && continue
      sleep 1
      xcrun simctl io "$udid" screenshot "$steps/$(basename "$f").png" >/dev/null 2>&1 || true
      touch "$f.seen"
    done
    sleep 1
  done
) &
watcher=$!
trap 'kill "$watcher" 2>/dev/null || true' EXIT

code="$work/code.txt"
if ! TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 TEST_RUNNER_LUMIA_DEV_NO_UNLOCK=1 \
  TEST_RUNNER_LUMIA_UITEST_STEPS="$steps" TEST_RUNNER_LUMIA_UITEST_REQUEST="$request" TEST_RUNNER_LUMIA_UITEST_LABEL="$label" TEST_RUNNER_LUMIA_UITEST_CODE="$code" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testApprovingADeviceFromThisPhone > "$work/test.log" 2>&1; then
  echo "FAIL: testApprovingADeviceFromThisPhone; see $work/test.log"
  xcrun simctl io "$udid" screenshot "$steps/failed.png" >/dev/null 2>&1 || true
  grep -E "error:|XCTAssert|failed" "$work/test.log" | head -10
  exit 1
fi
echo "ok: testApprovingADeviceFromThisPhone ($(ls "$steps" | grep -c '\.png$') screenshots in $steps)"

[ -s "$code" ] || { echo "FAIL: the test left no code"; exit 1; }
claimed=$("$cli" --data-dir "$new" identity claim "$(cat "$code")")
echo "$claimed"
identity=$(bound)
[ -n "$identity" ] || { echo "FAIL: $handle is bound to no identity"; exit 1; }
case "$claimed" in
  "linked to identity $identity "*) echo "ok: the CLI claimed the code into $handle's identity" ;;
  *) echo "FAIL: the CLI's claim is not into $identity"; exit 1 ;;
esac
roster=$("$cli" --data-dir "$new" identity show)
echo "$roster"
[ "$(printf '%s\n' "$roster" | grep -c '^  device ')" -eq 2 ] || { echo "FAIL: the roster the CLI holds is not the phone and itself"; exit 1; }
printf '%s\n' "$roster" | grep -q 'Native  device (this device)$' || { echo "FAIL: the CLI is not in the roster as added by a device"; exit 1; }
echo "ok: the roster the CLI holds is the phone and itself, added by the phone"
