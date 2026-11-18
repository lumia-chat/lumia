#!/bin/sh
# What is typed into a composer and not sent, kept by conversation through
# a switch to another, a quit or a crash (issue #23), in a simulator
# against a dev instance. The release CLI seeds a server whose #general
# has a question from another person and a message of the phone
# account's, and over HTTP a server DM between the two. The first test
# (LumiaUITests/SmokeUITests/testADraftIsKeptThroughASwitchAQuitAndASend)
# begins a reply in #general with "@ On" off and types without sending,
# finds the server DM's field empty and types there too, finds #general's
# words and reply as they were left, edits a message in place and finds
# the draft untouched, types more and lets it rest; this script then
# kills the app where it stands (SIGKILL, so nothing is told it is
# leaving), and the test launches it again and finds both conversations'
# words, sends #general's, finds its field empty after leaving, returning
# and another launch, and empties the DM's by hand. From the other side
# the script checks that the instance was sent none of it until the send
# (no message, and no typing notice for words put back in a field), and
# then the one message with its quote, silent. Then a personal
# conversation, on the same phone: the other person asks for one, the
# second test (testAPersonalConversationKeepsItsDraft) accepts, replies to
# what they then write and types without sending, the script kills the
# app again, and the test finds the words and the reply bar's name at the
# next launch, sends, and finds the field empty at another; the script
# reads the answer, once, as the other person's device.
# Screenshots land in .build/smoke-drafts/.
#
#   scripts/smoke-drafts.sh        # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4037 LUMIA_SIMULATOR="iPhone 18 Pro" LUMIA_CLI=/path/to/lumia-cli scripts/smoke-drafts.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 18 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-drafts}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"
bundle=chat.lumia
mine="$work/phone" # the account the phone signs in as
theirs="$work/other" # the other person

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }
fail() { echo "FAIL: $1"; exit 1; }

stamp=$(date +%s)
handle="phone$stamp"
other="other$stamp"
asked="Shall we ship on Friday?"
draft="Half a thought and more"
token() {
  curl -s -f -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])"
}
py() { python3 -c "import sys, json; d = json.load(sys.stdin); $1"; }
for d in "$mine" "$theirs"; do "$cli" --data-dir "$d" instance add "$instance" >/dev/null; done
"$cli" --data-dir "$mine" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$mine" server-create "Drafts smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$mine" channel-create "$server" general >/dev/null
general=$("$cli" --data-dir "$mine" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
invite=$("$cli" --data-dir "$mine" invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
"$cli" --data-dir "$theirs" account create --handle "$other" --password "$password" --invite "$invite" >/dev/null
"$cli" --data-dir "$mine" send "$general" "Mine to edit." >/dev/null
"$cli" --data-dir "$theirs" send "$general" "$asked" >/dev/null
# A server DM between the two, which the other person opens and writes in.
me=$("$cli" --data-dir "$mine" whoami | grep -oE "[0-9a-f-]{36}" | head -1)
tt=$(token "$other")
dm=$(curl -s -f -X POST -H "content-type: application/json" -H "accept: application/json" -H "authorization: Bearer $tt" \
  -d "{\"accountId\":\"$me\"}" "$instance/api/v1/servers/$server/dms" | py "d = d.get('channel', d); print(d['channelId'])")
"$cli" --data-dir "$theirs" send "$dm" "Got a minute after lunch?" >/dev/null
echo "seeded $handle and $other in server $server: #general $general, server DM $dm"

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

# The other person follows #general: a typing notice in what they are sent
# can only be the phone's, the instance never sending an account its own.
"$cli" --data-dir "$work/watch" instance add "$instance" >/dev/null
"$cli" --data-dir "$work/watch" login --handle "$other" --password "$password" >/dev/null
"$cli" --data-dir "$work/watch" tail "$general" > "$work/tail.log" 2>&1 &
tail=$!
uitest=""
follow=""
# shellcheck disable=SC2329
finish() {
  kill "$tail" 2>/dev/null || true
  [ -z "$follow" ] || kill "$follow" 2>/dev/null || true
  [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true
}
trap finish EXIT

# Runs one UI test beside this script: its steps under $work/<name>/, its log $work/<name>.log.
run() { # <test> <name> <the message a reply answers>
  name=$2
  steps="$work/$name"
  log="$work/$name.log"
  mkdir -p "$steps"
  TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
  TEST_RUNNER_LUMIA_UITEST_OTHER="$other" TEST_RUNNER_LUMIA_UITEST_ME="$handle" TEST_RUNNER_LUMIA_UITEST_LAST="$3" TEST_RUNNER_LUMIA_UITEST_STEPS="$steps" \
    xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
    "-only-testing:LumiaUITests/SmokeUITests/$1" > "$log" 2>&1 &
  uitest=$!
}
# Waits for the test to announce a step and takes its picture; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$steps/$1" ]; then
    xcrun simctl io "$udid" screenshot "$work/$name-failed-$1.png" >/dev/null 2>&1 || true
    fail "the UI test did not reach \"$1\"; see $log: $(grep -E "error|XCTAssert" "$log" | head -5)"
  fi
  xcrun simctl io "$udid" screenshot "$work/$name-$1.png" >/dev/null 2>&1 || true
}
seen() { touch "$steps/$1.seen"; }
passed() {
  wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
  uitest=""
}
# Kills the app where it stands, with the composer on screen: no leaving
# and no background, as after a crash, so only a save made while the
# words rested can have kept them.
crash() { # <what was typed>
  # The app's own entry, not its test runner's (chat.lumia.uitests.xctrunner).
  pid=$(xcrun simctl spawn "$udid" launchctl list | awk -v b="UIKitApplication:$bundle[" 'index($3, b) == 1 { print $1 }' | head -1)
  case "$pid" in ''|*[!0-9]*) fail "no running app to kill: \"$pid\"" ;; esac
  kill -9 "$pid"
  echo "killed the app (pid $pid) with \"$1\" typed and not sent"
}
tm=$(token "$handle")
# How many messages of a conversation begin with the words, as the instance has it.
sent() { # <channel> <words>
  curl -s -f -H "accept: application/json" -H "authorization: Bearer $tm" "$instance/api/v1/channels/$1/messages?limit=50" |
    py "print(len([m for m in d.get('messages', []) if m.get('body', '').startswith('$2')]))"
}
typing() { grep -c "^-- Typing" "$work/tail.log" || true; }

run testADraftIsKeptThroughASwitchAQuitAndASend channel "$asked"
reached switched 240
[ "$(sent "$general" "Half a thought")" = 0 ] || fail "the instance has #general's draft as a message"
echo "#general kept its words, its reply and \"@ On\" off through a switch to the server DM, whose field was empty"
seen switched

reached typed 120
[ "$(sent "$general" "Half a thought")" = 0 ] || fail "the instance has #general's draft as a message"
typed=$(typing)
[ "$typed" -gt 0 ] || fail "the other person was never told the phone was typing in #general, so the tail tells nothing; see $work/tail.log"
crash "$draft"
seen typed

reached relaunched 180
echo "launched again: #general has its words and its reply"
seen relaunched

reached sent 120
[ "$(typing)" = "$typed" ] || fail "the phone said it was typing in #general when a kept draft was put back in the field ($typed notices before the kill, $(typing) after)"
quoted=$(curl -s -f -H "accept: application/json" -H "authorization: Bearer $tm" "$instance/api/v1/channels/$general/messages?limit=50" | py "
m = [x for x in d.get('messages', []) if x.get('body') == '$draft']
q = m[0].get('replyTo', {}) if m else {}
print(len(m), '|', q.get('text', ''), '|', q.get('silent', False))")
[ "$quoted" = "1 | $asked | True" ] || fail "the instance does not have the draft sent once, quoting the question with @ On off: \"$quoted\""
echo "sent once, with the quote kept through the kill and \"@ On\" still off; the field stayed empty after leaving and returning, and no typing was said for words put back"
seen sent

reached emptied 180
[ "$(sent "$dm" "Elsewhere")" = 0 ] || fail "the instance has the server DM's draft as a message"
seen emptied
passed
echo "a channel and a server DM keep what is typed in them"

# A personal conversation, on the same phone, which set private messages
# up as it signed in: the other person, with an identity of their own now,
# asks for one, and the test accepts.
"$cli" --data-dir "$theirs" identity create >/dev/null
"$cli" --data-dir "$theirs" identity link-account --password "$password" >/dev/null
"$cli" --data-dir "$theirs" dm setup >/dev/null
"$cli" --data-dir "$theirs" dm request "$me" >/dev/null
dinner="Dinner on Thursday?"
answer="Thursday works"
run testAPersonalConversationKeepsItsDraft personal "$dinner"
reached accepted 240
# The phone made the conversation's group; the other person's device finds it and writes.
personal=""
for _ in $(seq 1 20); do
  personal=$("$cli" --data-dir "$theirs" dm open "$me" 2>/dev/null | grep -oE "[0-9a-f-]{36}\s+personal DM" | grep -oE "^[0-9a-f-]{36}" || true)
  [ -n "$personal" ] && "$cli" --data-dir "$theirs" dm send "$personal" "$dinner" >/dev/null 2>&1 && break
  sleep 2
done
[ -n "$personal" ] || fail "the other person has no personal conversation with the phone's account"
seen accepted

reached typed 180
crash "$answer"
seen typed

reached relaunched 180
echo "launched again: the personal conversation has its words, and its reply bar names the same person"
seen relaunched

reached sent 180
# The conversation as the other person's device reads it, once it has
# followed it (`dm messages` shows what the device holds and fetches nothing): the answer, once.
"$cli" --data-dir "$theirs" dm tail "$personal" > "$work/personal-tail.log" 2>&1 &
follow=$!
for _ in $(seq 1 15); do
  grep -q "$answer" "$work/personal-tail.log" && break
  sleep 2
done
kill "$follow" 2>/dev/null || true
wait "$follow" 2>/dev/null || true
follow=""
got=$("$cli" --data-dir "$theirs" dm messages "$personal" 2>/dev/null | grep -c "$answer" || true)
[ "$got" = 1 ] || fail "the other person does not read \"$answer\" once in the personal conversation: $got"
echo "sent once, as the other person's device reads it, and the field stayed empty at another launch"
seen sent
passed
echo "smoke-drafts: ok (a draft and its reply kept by conversation, in a channel, a server DM and a personal conversation, through a switch, an edit in place and a kill, gone once sent or emptied, and nothing of it sent before; screenshots under $work)"
