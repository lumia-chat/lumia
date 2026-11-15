#!/bin/sh
# Setting up private messages by hand (issue #20), against an instance
# built from this tree. Three accounts, each with a server so the debug
# build's LUMIA_DEV_SCREEN can open its screens, and a new phone for each
# (the app uninstalled and its app group's stores removed, so it holds no
# identity):
#   - one made with no identity, signed in with its password:
#     testPrivateMessagesAreSetUpAndThePhraseConfirmed expects the setup to
#     have been silent, Home's reminder, In a week, Devices' card, the
#     phrase, a wrong word, the phrase again and the right three; the script
#     reads the account's identity over HTTP before (none) and after (bound);
#   - one whose identity the CLI made: testThisPhoneIsBroughtInByRestoring
#     expects "Private messages need this phone", the sheet, and Restore
#     with the CLI's phrase; the script expects the account still bound to
#     that identity;
#   - another the CLI made: testStartingOverWithANewIdentity starts over
#     with the password; the script expects the account bound to a new identity.
# Each step announced is screenshotted into .build/smoke-private-messages/.
# The phone's unlock before the phrase is not driven: a simulator asks for
# a passcode the test cannot give, so LUMIA_DEV_NO_UNLOCK=1 skips it.
#
#   LUMIA_INSTANCE=http://localhost:4012 LUMIA_CLI=/path/to/lumia-cli scripts/smoke-private-messages.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-private-messages}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
# An account with a server; with "identity", the CLI makes it an identity
# and binds the account to it, printing the phrase.
seed() { # <handle> [identity]
  d="$work/cli-$1"
  "$cli" --data-dir "$d" instance add "$instance" >/dev/null
  "$cli" --data-dir "$d" account create --handle "$1" --password "$password" >/dev/null
  "$cli" --data-dir "$d" server-create "Private messages smoke" >/dev/null
  if [ "${2:-}" = identity ]; then
    "$cli" --data-dir "$d" identity create | sed -n '/recovery phrase/{n;p;}' > "$work/$1.phrase"
    "$cli" --data-dir "$d" identity link-account --password "$password" >/dev/null
  fi
}
# The identity the account is bound to, as the instance says it; empty for none.
bound() { # <handle>
  token=$(curl -s -f -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])")
  curl -s -f -H "accept: application/json" -H "authorization: Bearer $token" "$instance/api/v1/account/snapshot" |
    python3 -c "import sys, json; print((json.load(sys.stdin).get('account') or {}).get('identityId') or '')"
}

fresh="pmnew$stamp"; restorer="pmrestore$stamp"; starter="pmstart$stamp"
seed "$fresh"
seed "$restorer" identity
seed "$starter" identity
[ "$(wc -w < "$work/$restorer.phrase")" -eq 24 ] || { echo "FAIL: the CLI printed no phrase"; exit 1; }
[ -z "$(bound "$fresh")" ] || { echo "FAIL: $fresh has an identity before the phone"; exit 1; }
restorerID=$(bound "$restorer"); starterID=$(bound "$starter")
[ -n "$restorerID" ] && [ -n "$starterID" ] || { echo "FAIL: the CLI's accounts are not bound"; exit 1; }
echo "seeded $fresh, $restorer and $starter"

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

uitest=""
watcher=""
finish() {
  [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true
  [ -z "$watcher" ] || kill "$watcher" 2>/dev/null || true
}
trap finish EXIT

# One test on a new phone: each step it announces is screenshotted and
# answered with <step>.seen.
run() { # <test> <handle> <name> [phrase file]
  steps="$work/$3"
  mkdir -p "$steps"
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
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
  phrase=""
  [ -z "${4:-}" ] || phrase=$(cat "$4")
  if TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$2" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 TEST_RUNNER_LUMIA_DEV_NO_UNLOCK=1 \
    TEST_RUNNER_LUMIA_UITEST_STEPS="$steps" TEST_RUNNER_LUMIA_UITEST_PHRASE="$phrase" \
    xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
    -only-testing:"LumiaUITests/SmokeUITests/$1" > "$work/$3.log" 2>&1; then
    ok=1
  else
    ok=0
  fi
  kill "$watcher" 2>/dev/null || true
  watcher=""
  if [ "$ok" = 0 ]; then
    echo "FAIL: $1; see $work/$3.log"
    xcrun simctl io "$udid" screenshot "$steps/failed.png" >/dev/null 2>&1 || true
    grep -E "error:|XCTAssert|failed" "$work/$3.log" | head -10
    exit 1
  fi
  echo "ok: $1 ($(ls "$steps" | grep -c '\.png$') screenshots in $steps)"
}

run testPrivateMessagesAreSetUpAndThePhraseConfirmed "$fresh" setup
[ -n "$(bound "$fresh")" ] || { echo "FAIL: the phone set up nothing for $fresh"; exit 1; }
echo "ok: $fresh is bound to the phone's identity"

run testThisPhoneIsBroughtInByRestoring "$restorer" restore "$work/$restorer.phrase"
[ "$(bound "$restorer")" = "$restorerID" ] || { echo "FAIL: restoring moved $restorer to another identity"; exit 1; }
echo "ok: $restorer is still its identity's, now on the phone"

run testStartingOverWithANewIdentity "$starter" startover
now=$(bound "$starter")
[ -n "$now" ] && [ "$now" != "$starterID" ] || { echo "FAIL: starting over left $starter on its old identity"; exit 1; }
echo "ok: $starter started over with a new identity"
