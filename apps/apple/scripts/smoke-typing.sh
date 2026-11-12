#!/bin/sh
# A server's channel with someone else in it, in a simulator against a dev
# instance: what one person sends close together is one block, and the
# channel says who is typing (protocol 6.7). The release CLI is the other
# person: it joins the phone account's server, sends three messages in a
# row, follows the channel, and types when the UI test asks. The test
# (LumiaUITests/SmokeUITests/testBlocksAndTypingInAChannel) reads the
# screen as assistive technology does and types into the composer; this
# script checks from the other side that the phone's typing arrived.
# Screenshots land in .build/smoke-typing/.
#
#   scripts/smoke-typing.sh          # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-typing.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli="$root/apps/core/target/release/lumia-cli"
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-typing}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia
mine="$work/phone" # the account the phone signs in as
theirs="$work/other" # the other person

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
handle="phone$stamp"
other="other$stamp"
last="Three, under the same name."
for d in "$mine" "$theirs"; do "$cli" --data-dir "$d" instance add "$instance" >/dev/null; done
"$cli" --data-dir "$mine" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$mine" server-create "Typing smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$mine" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$mine" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
invite=$("$cli" --data-dir "$mine" invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
"$cli" --data-dir "$theirs" account create --handle "$other" --password "$password" --invite "$invite" >/dev/null
"$cli" --data-dir "$mine" send "$channel" "Anyone here?" >/dev/null
"$cli" --data-dir "$theirs" send "$channel" "One." >/dev/null
"$cli" --data-dir "$theirs" send "$channel" "Two, a moment later." >/dev/null
"$cli" --data-dir "$theirs" send "$channel" "$last" >/dev/null
echo "seeded $handle and $other in server $server, channel $channel"

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
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
# The app group's container keeps the stores across installs; start clean.
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true

# The other person follows the channel: a typing notice in what they are
# sent can only be the phone's, the instance never sending an account its own.
"$cli" --data-dir "$work/watch" instance add "$instance" >/dev/null
"$cli" --data-dir "$work/watch" login --handle "$other" --password "$password" >/dev/null
"$cli" --data-dir "$work/watch" tail "$channel" > "$work/tail.log" 2>&1 &
tail=$!
uitest=""
# shellcheck disable=SC2329
finish() {
  kill "$tail" 2>/dev/null || true
  [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true
}
trap finish EXIT

TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_SCREEN=channel TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_OTHER="$other" TEST_RUNNER_LUMIA_UITEST_ROWS=3 TEST_RUNNER_LUMIA_UITEST_LAST="$last" TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testBlocksAndTypingInAChannel > "$work/uitest.log" 2>&1 &
uitest=$!

# Waits for the test to announce a step; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && return 0
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  [ -f "$work/steps/$1" ] && return 0
  echo "FAIL: the UI test did not reach \"$1\"; see $work/uitest.log"
  xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
  grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10
  exit 1
}

reached rows 240
xcrun simctl io "$udid" screenshot "$work/blocks.png" >/dev/null 2>&1 || true
echo "the block's rows each say whose they are"

# The other person types until the phone has typed too: one word of it
# lasts eight seconds, and the instance takes one in three.
said=0
until [ -f "$work/steps/typed" ]; do
  kill -0 "$uitest" 2>/dev/null || break
  "$cli" --data-dir "$theirs" typing "$channel" >/dev/null 2>&1 || true
  said=$((said + 1))
  if [ -f "$work/steps/shown" ] && [ ! -f "$work/typing.png" ]; then
    xcrun simctl io "$udid" screenshot "$work/typing.png" >/dev/null 2>&1 || true
  fi
  [ "$said" -lt 60 ] || break
  sleep 2
done
reached typed 5
echo "the phone said $other is typing (after $said of their words of it)"

# The phone's own typing, as the other person was sent it.
seen=0
for _ in $(seq 1 10); do
  seen=$(grep -c "^-- Typing" "$work/tail.log" || true)
  [ "$seen" -gt 0 ] && break
  sleep 1
done
[ "$seen" -gt 0 ] || { echo "FAIL: the other person was not told the phone is typing; see $work/tail.log"; tail -5 "$work/tail.log"; exit 1; }
echo "the other person was told the phone is typing ($seen notice for one burst of keys)"

reached quiet 60
xcrun simctl io "$udid" screenshot "$work/quiet.png" >/dev/null 2>&1 || true
wait "$uitest" || { echo "FAIL: the UI test did not pass; see $work/uitest.log"; grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10; exit 1; }
uitest=""
echo "smoke-typing: ok (a block's rows say whose they are, the channel said who was typing and stopped by itself, and the phone's typing reached the other person; screenshots under $work)"
