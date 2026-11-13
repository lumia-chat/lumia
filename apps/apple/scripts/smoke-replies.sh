#!/bin/sh
# Quote replies, reactions, mentions and read state by hand (architecture
# 6.6), in a simulator against an instance built from this tree. The
# phone's account owns a server; Ada Lovelace, the release CLI and the
# instance's HTTP API (the CLI sends no quote or reaction), quotes one of
# its messages, quotes one it then deletes, reacts to it, mentions it in
# #general and in #design, and writes in #random; Bo Diddley is in the
# server and not in #general. The script draws what the LUMIA_DEV_SCREEN
# values server, channel, mentions and homementions open, light and dark,
# then runs testRepliesReactionsMentionsAndReads and reads the instance at
# each step: #design read once its newest message was on screen, a reply
# made from a long press carrying its quote, a reaction added and taken
# away again, a mention picked from the @ list of someone not in the channel
# and "Add to #general" adding them. Last, the read wake: three notifications
# delivered to the app in the background, #design read from elsewhere, and
# the read push through the debug build's lumia://dev/read-push (simctl push
# delivers no silent push); #design's notification goes and #general's stays.
# Screenshots in .build/smoke-replies/.
#
#   scripts/smoke-replies.sh       # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4037 LUMIA_SIMULATOR="iPhone 18 Pro" LUMIA_CLI=/path/to/lumia-cli scripts/smoke-replies.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 18 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-replies}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
owner="robin$stamp"
c() { d=$1; shift; "$cli" --data-dir "$work/$d" "$@"; }
person() { # <dir> <handle> <display name> [--invite link]
  d=$1; h=$2; n=$3; shift 3
  c "$d" instance add "$instance" >/dev/null
  c "$d" account create --handle "$h" --password "$password" "$@" >/dev/null
  c "$d" identity create >/dev/null
  c "$d" identity link-account --password "$password" >/dev/null
  c "$d" identity profile --display-name "$n" >/dev/null
}
token() {
  curl -s -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])"
}
api() { # <token> <method> <path> [json]
  if [ $# -ge 4 ]; then
    curl -s -f -X "$2" -H "content-type: application/json" -H "accept: application/json" -H "authorization: Bearer $1" -d "$4" "$instance/api/v1$3"
  else
    curl -s -f -X "$2" -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1$3"
  fi
}
py() { python3 -c "import sys, json; d = json.load(sys.stdin); $1"; }
uuid() { grep -oE "[0-9a-f-]{36}" | head -1; }
# A client-minted command id: a UUIDv7.
uuid7() { python3 -c "
import os, time, uuid
b = bytearray(int(time.time() * 1000).to_bytes(6, 'big') + os.urandom(10))
b[6] = 0x70 | (b[6] & 0x0f); b[8] = 0x80 | (b[8] & 0x3f)
print(uuid.UUID(bytes=bytes(b)))"; }
# A command over HTTP (protocol: POST /channels/{id}/commands), as <token>; prints its id.
command() { # <token> <channel> <op json>
  id=$(uuid7)
  api "$1" POST "/channels/$2/commands" "{\"commandId\":\"$id\",\"channelId\":\"$2\",$3}" >/dev/null
  echo "$id"
}
say() { # <token> <channel> <body> [extra json for sendMessage]
  command "$1" "$2" "\"sendMessage\":{\"body\":$(printf '%s' "$3" | python3 -c 'import sys, json; print(json.dumps(sys.stdin.read()))')${4:+,$4}}"
}
fail() { echo "FAIL: $*"; exit 1; }

person owner "$owner" "Robin Owner"
server=$(c owner server-create "Acme Works" | uuid)
c owner channel-create "$server" general >/dev/null
general=$(c owner channels "$server" | uuid)
invite=$(c owner invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
person ada "ada$stamp" "Ada Lovelace" --invite "$invite"
person bo "bo$stamp" "Bo Diddley" --invite "$invite"
to=$(token "$owner"); ta=$(token "ada$stamp"); tb=$(token "bo$stamp")
owner_id=$(c owner whoami | uuid); ada_id=$(c ada whoami | uuid); bo_id=$(c bo whoami | uuid)
channel_id() { py "d = d.get('channel', d); print(d['channelId'])"; }
design=$(api "$to" POST /channels "{\"serverId\":\"$server\",\"kind\":\"TEXT\",\"name\":\"design\"}" | channel_id)
random=$(api "$to" POST /channels "{\"serverId\":\"$server\",\"kind\":\"TEXT\",\"name\":\"random\"}" | channel_id)
api "$ta" POST "/channels/$design/join" >/dev/null
api "$ta" POST "/channels/$random/join" >/dev/null
api "$tb" POST "/channels/$general/leave" >/dev/null

# #general: the phone's message, Ada's quote of it, a quote of one deleted, a mention, reactions.
shipped=$(say "$to" "$general" "Shipped on Friday.")
draft=$(say "$to" "$general" "A draft I will take back.")
say "$ta" "$general" "Morning all, the review is at ten." >/dev/null
say "$ta" "$general" "Nice work, that was quick." "\"replyTo\":{\"messageId\":\"$shipped\",\"text\":\"Shipped on Friday.\"}" >/dev/null
say "$ta" "$general" "What did this say?" "\"replyTo\":{\"messageId\":\"$draft\",\"text\":\"A draft\"}" >/dev/null
command "$to" "$general" "\"deleteMessage\":{\"messageId\":\"$draft\"}" >/dev/null
command "$ta" "$general" "\"addReaction\":{\"messageId\":\"$shipped\",\"reaction\":\"👍\"}" >/dev/null
command "$ta" "$general" "\"addReaction\":{\"messageId\":\"$shipped\",\"reaction\":\"🎉\"}" >/dev/null
# A mention of the phone's account in <body>, with its UTF-8 byte range (the CLI's has none).
mentioning() { # <body>
  python3 -c "
import sys
body, word = sys.argv[1], '@' + sys.argv[2]
start = len(body[:body.index(word)].encode())
print('\"mentions\":[{\"accountId\":\"%s\",\"start\":%d,\"end\":%d}]' % (sys.argv[3], start, start + len(word.encode())))" "$1" "$owner" "$owner_id"
}
morning="Can you look at the type scale, @$owner?"
say "$ta" "$general" "$morning" "$(mentioning "$morning")" >/dev/null
# #design: two from Ada, the second for the phone's account: unread, one mention.
say "$ta" "$design" "The mockups are in the drive." >/dev/null
which="@$owner which one do you like?"
say "$ta" "$design" "$which" "$(mentioning "$which")" >/dev/null
# #random: unread for the dark picture of its "New messages" line.
for n in 1 2 3; do say "$ta" "$random" "Random thought number $n." >/dev/null; done
echo "seeded $owner owning Acme Works ($server), with Ada and Bo"

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
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
xcrun simctl install "$udid" "$app"

shoot() { # <picture> <screen> [KEY=VALUE...]
  picture=$1; screen=$2; shift 2
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  for kv in "$@"; do export "SIMCTL_CHILD_$kv"; done
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$owner" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
  SIMCTL_CHILD_LUMIA_DEV_SCREEN="$screen" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep 10
  xcrun simctl io "$udid" screenshot "$work/$picture.png" >/dev/null 2>&1
  for kv in "$@"; do unset "SIMCTL_CHILD_${kv%%=*}"; done
  echo "  $picture: $work/$picture.png"
}
unread_of() { api "$to" GET /account/snapshot | py "print(next((f\"{r.get('unread', 0)} {r.get('mentions', 0)}\" for r in d.get('readStates', []) if r['channelId'] == '$1'), '0 0'))"; }
[ "$(unread_of "$design")" = "2 1" ] || fail "#design is not unread with one mention before the phone looks: $(unread_of "$design")"
shoot server server LUMIA_DEV_APPEARANCE=light
shoot server-dark server LUMIA_DEV_APPEARANCE=dark
shoot general channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_APPEARANCE=light
shoot general-dark channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_APPEARANCE=dark
shoot random-dark channel LUMIA_DEV_CHANNEL=random LUMIA_DEV_APPEARANCE=dark
shoot mentions mentions LUMIA_DEV_APPEARANCE=light
shoot mentions-dark mentions LUMIA_DEV_APPEARANCE=dark
shoot homementions homementions LUMIA_DEV_APPEARANCE=light
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
[ "$(unread_of "$random")" = "0 0" ] || fail "#random is still unread after it was on screen: $(unread_of "$random")"
echo "#random was read when it was opened"

log="$work/testRepliesReactionsMentionsAndReads.log"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$owner" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_OTHER="bo$stamp" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testRepliesReactionsMentionsAndReads > "$log" 2>&1 &
uitest=$!
trap 'kill "$uitest" 2>/dev/null || true' EXIT

reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    fail "the UI test did not reach \"$1\"; see $log: $(grep -E "error|XCTAssert" "$log" | head -5)"
  fi
  xcrun simctl io "$udid" screenshot "$work/step-$1.png" >/dev/null 2>&1 || true
}
seen() { touch "$work/steps/$1.seen"; }
messages() { api "$to" GET "/channels/$1/messages?limit=50"; }

reached sidebar 180
seen sidebar
reached newline 60
seen newline
reached read 60
[ "$(unread_of "$design")" = "0 0" ] || fail "#design is still unread once its newest message was on screen: $(unread_of "$design")"
echo "#design: unread and a mention before, read once its newest message was on screen"
seen read

reached actions 60
seen actions
reached replied 60
quoted=$(messages "$general" | py "
m = [x for x in d.get('messages', []) if x.get('body') == 'On it, thanks.']
print(m[0].get('replyTo', {}).get('text', '') if m else '')")
[ "$quoted" = "Morning all, the review is at ten." ] || fail "the reply made from the long press does not quote Ada's message: \"$quoted\""
echo "a reply by long press carries its quote"
seen replied

reached reacted 60
mine=$(messages "$general" | py "
m = [x for x in d.get('messages', []) if x.get('messageId') == '$shipped'][0]
print(' '.join(sorted(r.get('reaction', '') + ':' + str(r.get('count', 0)) for r in m.get('reactions', []))))")
echo "reactions on the phone's message after the chip's tap: $mine"
case "$mine" in *"👍:2"*) ;; *) fail "the 👍 chip did not add this account's reaction: $mine" ;; esac
seen reacted
reached unreacted 60
mine=$(messages "$general" | py "
m = [x for x in d.get('messages', []) if x.get('messageId') == '$shipped'][0]
print(' '.join(sorted(r.get('reaction', '') + ':' + str(r.get('count', 0)) for r in m.get('reactions', []))))")
case "$mine" in *"👍:1"*) ;; *) fail "the 👍 chip did not take this account's reaction away again: $mine" ;; esac
echo "a reaction toggled on and off by its chip"
seen unreacted

reached mentioned 60
mention=$(messages "$general" | py "
m = [x for x in d.get('messages', []) if x.get('body', '').startswith('@bo$stamp')]
print(' '.join(x.get('accountId', '') for x in (m[0].get('mentions', []) if m else [])))")
[ "$mention" = "$bo_id" ] || fail "the message sent with a pick from the @ list does not mention Bo: \"$mention\""
echo "a mention picked from the @ list, of someone not in the channel"
seen mentioned
reached added 60
api "$to" GET "/channels/$general/members" | py "import sys; sys.exit(0 if '$bo_id' in d.get('accountIds', []) else 1)" || fail "Add to #general did not add Bo"
echo "the bar's Add to #general added Bo"
seen added

reached mentionspage 60
seen mentionspage
wait "$uitest" || fail "the UI test did not pass; see $log: $(grep -E "error|XCTAssert" "$log" | head -5)"
trap - EXIT

# The read wake: notifications delivered to the app in the background, #design
# read from elsewhere, then the read push (the debug build's stand-in for it,
# since simctl delivers no silent push). #general's notification stays.
log="$work/testTheReadWakeTakesAwayWhatIsRead.log"
read_words="Design, wake $stamp."
unread_words="General, wake $stamp."
badge_of() { api "$to" GET /account/snapshot | py "print(sum(r.get('mentions', 0) for r in d.get('readStates', [])))"; }
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$owner" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_READ="$read_words" TEST_RUNNER_LUMIA_UITEST_UNREAD="$unread_words" \
TEST_RUNNER_LUMIA_UITEST_WAKE="lumia://dev/read-push?badge=1" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testTheReadWakeTakesAwayWhatIsRead > "$log" 2>&1 &
uitest=$!
trap 'kill "$uitest" 2>/dev/null || true' EXIT
reached wake-ready 180
say "$ta" "$design" "$read_words" >/dev/null
general_words="@$owner $unread_words"
say "$ta" "$general" "$general_words" "$(mentioning "$general_words")" >/dev/null
notify() { # <name> <channel> <body>
  # As the extension leaves it: the conversation in the content (simctl runs no extension).
  printf '{"aps":{"alert":{"title":"Ada Lovelace","body":"%s"},"badge":2},"channel_id":"%s","kind":"mention"}' "$3" "$2" > "$work/$1.json"
  xcrun simctl push "$udid" "$bundle" "$work/$1.json" >/dev/null
}
# Notification Center stacks an app's notifications under its newest, which is what it shows: #general's last.
printf '{"aps":{"alert":{"title":"Lumia","body":"New message"},"badge":2}}' > "$work/generic.json"
xcrun simctl push "$udid" "$bundle" "$work/generic.json" >/dev/null
notify design "$design" "$read_words"
notify general "$general" "$unread_words"
seq=$(api "$to" GET /account/snapshot | py "print(next(c.get('lastSeq', 0) for c in d['channels'] if c['channelId'] == '$design'))")
api "$to" POST "/channels/$design/read" "{\"seq\":$seq}" >/dev/null
[ "$(unread_of "$design")" = "0 0" ] || fail "#design was not read from elsewhere"
echo "three notifications delivered with the app in the background, #design read from elsewhere (badge $(badge_of))"
seen wake-ready
reached woken 120
seen woken
wait "$uitest" || fail "the read wake test did not pass; see $log: $(grep -E "error|XCTAssert" "$log" | head -5)"
trap - EXIT
said=$(xcrun simctl spawn "$udid" log show --last 3m --info --predicate 'subsystem == "chat.lumia" AND category == "push"' --style compact 2>/dev/null |
  grep -E "read push|removing" | sed 's/^.*\] //' || true)
echo "$said" | sed 's/^/  the app: /'
# The app was not running: what went, the wake took (the generic notice stays while the badge is not 0).
echo "$said" | grep -q "read push: badge 1, 1 of 3 delivered notifications removed" || fail "the read wake did not take away #design's notification alone"
echo "the read wake took #design's notification away and left #general's and the generic one"
echo "smoke-replies: ok (screenshots under $work)"
