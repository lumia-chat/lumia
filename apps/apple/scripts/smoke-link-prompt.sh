#!/bin/sh
# Linking by a prompt and six matching digits by hand (identity spec 4.2.1),
# against an instance built from this tree, on two new phones. The release
# CLI makes an account with no identity. The first phone signs in with its
# password, so private messages are set up silently and it holds the
# identity; testAPromptIsAnsweredOnThisPhone drives it. Once it says it is
# ready, the second phone signs in to the same account with its password
# and opens the link screen (LUMIA_DEV_SCREEN=link), which starts linking
# by itself and is never touched. The prompt comes on the first phone, is
# continued, both phones show their digits (the test leaves the first's in
# a file; the script prints them and screenshots the second's), "They
# match" approves (the unlock skipped, LUMIA_DEV_NO_UNLOCK=1), and the
# first counts two devices. The second phone, linked, goes on into the app
# by itself; the script then expects the account bound to one identity and
# screenshots it. Each step is screenshotted on both phones into
# .build/smoke-link-prompt/. The unlock, "Not me", "They don't match",
# Cancel and "Link another way" are not driven here (the kit's test covers
# declining against an instance).
#
#   LUMIA_INSTANCE=http://localhost:4053 LUMIA_CLI=/path/to/lumia-cli \
#   LUMIA_SIMULATOR="iPhone 17 Pro" LUMIA_SIMULATOR_NEW="iPhone 17" scripts/smoke-link-prompt.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
simulator_new=${LUMIA_SIMULATOR_NEW:-iPhone 15}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-link-prompt}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"
bundle=chat.lumia
# What the second phone calls itself: UIDevice's model.
label="iPhone"

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

handle="prompt$(date +%s)"
owner="$work/cli-owner"
"$cli" --data-dir "$owner" instance add "$instance" >/dev/null
"$cli" --data-dir "$owner" account create --handle "$handle" --password "$password" >/dev/null
"$cli" --data-dir "$owner" server-create "Prompt smoke" >/dev/null
echo "seeded $handle"

# The identity the account is bound to, as the instance says it; empty for none.
bound() {
  token=$(curl -s -f -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$handle\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])")
  curl -s -f -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/account/snapshot" |
    python3 -c "import sys, json; print((json.load(sys.stdin).get('account') or {}).get('identityId') or '')"
}

# A simulator by name, booted; its udid.
pick() {
  xcrun simctl list devices available -j | python3 -c "
import json,sys
all=[d for r in json.load(sys.stdin)['devices'].values() for d in r]
devs=[d for d in all if d['name']==sys.argv[1]]
devs.sort(key=lambda d: d['state']!='Booted')
print(devs[0]['udid'] if devs else '')" "$1"
}
udid=$(pick "$simulator")
new=$(pick "$simulator_new")
[ -n "$udid" ] && [ -n "$new" ] && [ "$udid" != "$new" ] || { echo "two simulators needed: '$simulator' and '$simulator_new'"; exit 1; }
for d in "$udid" "$new"; do
  xcrun simctl boot "$d" 2>/dev/null || true
  xcrun simctl bootstatus "$d" -b >/dev/null
  # A new phone: no identity held from an earlier run.
  xcrun simctl terminate "$d" "$bundle" 2>/dev/null || true
  xcrun simctl uninstall "$d" "$bundle" 2>/dev/null || true
  rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$d/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
done
xcrun simctl install "$new" "$app"

steps="$work/steps"
ready="$work/ready"
digits="$work/digits.txt"
mkdir -p "$steps"
(
  launched=""
  while :; do
    # The first phone holds the identity: the second starts linking.
    if [ -z "$launched" ] && [ -f "$ready" ]; then
      SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$handle" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
        SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 SIMCTL_CHILD_LUMIA_DEV_SCREEN=link xcrun simctl launch "$new" "$bundle" >/dev/null
      launched=1
    fi
    for f in "$steps"/*; do
      case "$f" in *.seen|*.png|*'*') continue ;; esac
      [ -f "$f.seen" ] && continue
      sleep 1
      xcrun simctl io "$udid" screenshot "$steps/$(basename "$f").png" >/dev/null 2>&1 || true
      xcrun simctl io "$new" screenshot "$steps/$(basename "$f")-new.png" >/dev/null 2>&1 || true
      touch "$f.seen"
    done
    sleep 1
  done
) &
watcher=$!
trap 'kill "$watcher" 2>/dev/null || true' EXIT

if ! TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 TEST_RUNNER_LUMIA_DEV_NO_UNLOCK=1 \
  TEST_RUNNER_LUMIA_UITEST_STEPS="$steps" TEST_RUNNER_LUMIA_UITEST_LABEL="$label" TEST_RUNNER_LUMIA_UITEST_READY="$ready" TEST_RUNNER_LUMIA_UITEST_DIGITS="$digits" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testAPromptIsAnsweredOnThisPhone > "$work/test.log" 2>&1; then
  echo "FAIL: testAPromptIsAnsweredOnThisPhone; see $work/test.log"
  xcrun simctl io "$udid" screenshot "$steps/failed.png" >/dev/null 2>&1 || true
  xcrun simctl io "$new" screenshot "$steps/failed-new.png" >/dev/null 2>&1 || true
  grep -E "error:|XCTAssert|failed" "$work/test.log" | head -10
  exit 1
fi
echo "ok: testAPromptIsAnsweredOnThisPhone ($(ls "$steps" | grep -c '\.png$') screenshots in $steps)"
echo "the first phone showed $(cat "$digits"); compare.png and compare-new.png are both phones at that moment"

identity=$(bound)
[ -n "$identity" ] || { echo "FAIL: $handle is bound to no identity"; exit 1; }
echo "ok: $handle is bound to $identity"
# The second phone, linked, has gone on into the app by itself.
sleep 5
xcrun simctl io "$new" screenshot "$work/after-new.png" >/dev/null 2>&1 || true
echo "the second phone after linking: $work/after-new.png"
