#!/bin/sh
# A server's own emoji by hand (specs/emoji.md, plan step 12), in a
# simulator against an instance built from this tree with a blob store (a
# dev or scratch instance). It seeds the boards' Acme with the CLI and the
# instance's HTTP API: the phone's account owning it, Maya, Devon and Priya
# in it, twelve emoji uploaded over HTTP (two of them animated GIFs, one
# wide and one tall, all made by scripts/emoji-picture.py), :oldlogo: used
# in a message and as a reaction and then removed, and a morning in
# #general whose messages use Acme's emoji (their refs found here as the
# core finds them) with reactions. It draws each screen light and dark into
# .build/smoke-emoji/, to be looked at beside the "Emoji …" boards:
# #general (and with the animated ones still, as under Reduce Motion), the
# picker from Add reaction, its search, the composer's picker, the `:` list
# over the keyboard, Server settings' sections, Emoji, a row's sheet,
# Remove's question, a rename refused, the upload form on a file as Files
# hands one over, and with LUMIA_SMOKE_DB (the instance's database, for
# psql) the section at its limit. Then LumiaUITests/SmokeUITests/testEmojiByHand
# drives the owner's phone and this script reads the instance at each step:
# a picture uploaded from the photo picker through Upload emoji, a name
# Acme has refused and another typed, the picker's server section and search
# reacting with a custom emoji, the composer's emoji button putting
# :name: in the field, the `:` list completing one, the message sent with
# its ref and drawn as a picture, a rename refused for a name taken and
# then made, and a removal asked first.
#
#   scripts/smoke-emoji.sh   # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4033 LUMIA_SIMULATOR="iPhone 17" LUMIA_CLI=/path/to/lumia-cli scripts/smoke-emoji.sh
#   LUMIA_SMOKE_DB=postgres://postgres:postgres@localhost/lumia_dev scripts/smoke-emoji.sh   # the limit's picture too
#   LUMIA_SMOKE_PICTURES=only scripts/smoke-emoji.sh      # the pictures, and no test
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-emoji}
rm -rf "$work" && mkdir -p "$work/steps" "$work/pictures"
password="correct-horse-battery-staple"
bundle=chat.lumia

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
owner="ryan$stamp"
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
fail() { echo "FAIL: $*"; exit 1; }

person owner "$owner" "Ryan Winchester"
server=$(c owner server-create "Acme" | uuid)
c owner channel-create "$server" general >/dev/null
general=$(c owner channels "$server" | uuid)
invite=$(c owner invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
to=$(token "$owner")
person maya "maya$stamp" "Maya Chen" --invite "$invite"
person devon "devon$stamp" "Devon Park" --invite "$invite"
person priya "priya$stamp" "Priya Natarajan" --invite "$invite"
tm=$(token "maya$stamp"); td=$(token "devon$stamp"); tp=$(token "priya$stamp")

# Acme's emoji, oldest first, uploaded by whoever the boards say (the owner
# for all of them here: only the owner holds Manage emoji). Each picture is
# one of scripts/emoji-picture.py's or the core's animated fixture.
picture() { python3 "$here/emoji-picture.py" "$work/pictures/$1.png" "$2" "$3" "$4"; }
picture acme disc 79,91,213 255,255,255
picture lgtm wide 46,158,91 191,240,210
picture shipit ring 168,105,58 0,0,0
picture plusone disc 47,111,222 255,255,255
picture coffee disc 243,239,231 107,66,38
picture blobcat disc 242,164,58 43,29,16
picture thisisfine disc 255,122,26 255,210,63
picture party_blob ring 255,201,51 0,0,0
picture sadparrot disc 138,155,176 124,196,255
picture hype tall 255,92,138 255,210,63
picture oldlogo ring 120,120,120 0,0,0
picture heart disc 79,91,213 255,255,255
python3 "$here/emoji-picture.py" "$work/pictures/partyparrot.gif" spin 61,190,90 229,72,77
python3 "$here/emoji-picture.py" "$work/pictures/parrotwave.gif" spin 74,139,234 245,184,61
upload() { # <token> <name> <file> <type>; prints the emoji's id
  curl -s -f -X POST -H "content-type: $4" -H "accept: application/json" -H "authorization: Bearer $1" \
    --data-binary "@$3" "$instance/api/v1/servers/$server/emoji?name=$2" | py "d = d.get('emoji', d); print(d['emojiId'])"
}
oldlogo=$(upload "$to" oldlogo "$work/pictures/oldlogo.png" image/png)
for name in acme lgtm shipit plusone coffee blobcat thisisfine; do upload "$to" "$name" "$work/pictures/$name.png" image/png >/dev/null; done
upload "$to" partyparrot "$work/pictures/partyparrot.gif" image/gif >/dev/null
upload "$to" parrotwave "$work/pictures/parrotwave.gif" image/gif >/dev/null
for name in party_blob sadparrot hype; do upload "$to" "$name" "$work/pictures/$name.png" image/png >/dev/null; done
emoji_list() { api "$to" GET "/servers/$server/emoji" | py "$1"; }
emoji_id() { emoji_list "print(next((e['emojiId'] for e in d.get('emoji', []) if e['name'] == '$1'), ''))"; }
emoji_names() { emoji_list "print(' '.join(e['name'] for e in d.get('emoji', [])))"; }
[ "$(emoji_list "print(len(d.get('emoji', [])))")" = 13 ] || fail "Acme has $(emoji_names) after the uploads"
echo "uploaded Acme's twelve and :oldlogo:: $(emoji_names)"

# #general. Each message is sent over HTTP with its refs, found here as the
# core finds them for a send (every `:name:` of a live emoji, by UTF-8
# offsets); the phone's own sends find theirs in its core.
uuid7() { python3 -c "
import os, time, uuid
b = bytearray(int(time.time() * 1000).to_bytes(6, 'big') + os.urandom(10))
b[6] = 0x70 | (b[6] & 0x0f); b[8] = 0x80 | (b[8] & 0x3f)
print(uuid.UUID(bytes=bytes(b)))"; }
listed=$(api "$to" GET "/servers/$server/emoji")
say() { # <token> <body>; prints the message's id
  op=$(LISTED="$listed" BODY="$2" python3 -c "
import json, os, re
names = {e['name']: e['emojiId'] for e in json.loads(os.environ['LISTED']).get('emoji', [])}
body = os.environ['BODY']
refs = [{'emojiId': names[m.group(1).decode()], 'start': m.start(), 'end': m.end()} for m in re.finditer(rb':([a-z0-9_]{2,32}):', body.encode()) if m.group(1).decode() in names]
print(json.dumps({'body': body, 'emoji': refs}))")
  api "$1" POST "/channels/$general/commands" "{\"commandId\":\"$(uuid7)\",\"channelId\":\"$general\",\"sendMessage\":$op}" >/dev/null
  sleep 1
  api "$to" GET "/channels/$general/messages?limit=50" | BODY="$2" py "import os; print(next(m['messageId'] for m in reversed(d.get('messages', [])) if m.get('body') == os.environ['BODY']))"
}
badge=$(say "$td" "The launch badge is up: :oldlogo: :shipit:")
shipped=$(say "$tm" "Onboarding shipped this morning :partyparrot: thanks, all.")
lgtm=$(say "$td" ":lgtm: on the copy changes. Merging after lunch :coffee:")
say "$tp" "The build is red again :thisisfine:" >/dev/null
fixed=$(say "$to" "Fixed, it was the cache. Green now :party_blob: :blobcat:")
refs() { api "$to" GET "/channels/$general/messages?limit=50" | py "m = next((m for m in d.get('messages', []) if m['messageId'] == '$1'), {}); print(len(m.get('emoji', [])))"; }
[ "$(refs "$shipped")" = 1 ] || fail "Maya's message carries $(refs "$shipped") emoji refs, not 1"
command_as() { # <who> <op json>
  case $1 in owner) t=$to ;; maya) t=$tm ;; devon) t=$td ;; *) t=$tp ;; esac
  api "$t" POST "/channels/$general/commands" "{\"commandId\":\"$(uuid7)\",\"channelId\":\"$general\",$2}" >/dev/null
}
react_http() { command_as "$1" "\"addReaction\":{\"messageId\":\"$2\",\"emojiId\":\"$3\"}"; }
react_unicode() { command_as "$1" "\"addReaction\":{\"messageId\":\"$2\",\"reaction\":\"$3\"}"; }
# shellcheck disable=SC2154
{
react_http maya "$badge" "$oldlogo"
react_http priya "$badge" "$oldlogo"
react_unicode maya "$badge" "🚀"
react_http devon "$shipped" "$(emoji_id partyparrot)"
react_http priya "$shipped" "$(emoji_id partyparrot)"
react_unicode priya "$shipped" "🎉"
react_http priya "$shipped" "$(emoji_id hype)"
react_http maya "$lgtm" "$(emoji_id lgtm)"
react_http priya "$fixed" "$(emoji_id blobcat)"
react_unicode priya "$fixed" "👍"
}
# :oldlogo: goes: its message shows its name, its reactions stay as :oldlogo:.
curl -s -f -X DELETE -H "authorization: Bearer $to" "$instance/api/v1/servers/$server/emoji/$oldlogo" >/dev/null
[ "$(emoji_list "print(len(d.get('emoji', [])))")" = 12 ] || fail "Acme has $(emoji_names) after :oldlogo: was removed"
echo "seeded $owner owning Acme ($server): twelve emoji, :oldlogo: removed, a morning in #general"

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
fresh() {
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
}
fresh
xcrun simctl install "$udid" "$app"
signed=$owner
shoot() { # <picture> <screen> [KEY=VALUE...]: once light and once dark
  picture=$1; screen=$2; shift 2
  for kv in "$@"; do export "SIMCTL_CHILD_$kv"; done
  for look in light dark; do
    xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
    SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$signed" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
    SIMCTL_CHILD_LUMIA_DEV_SCREEN="$screen" SIMCTL_CHILD_LUMIA_DEV_APPEARANCE="$look" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 xcrun simctl launch "$udid" "$bundle" >/dev/null
    sleep "${LUMIA_SMOKE_WAIT:-9}"
    xcrun simctl io "$udid" screenshot "$work/$picture-$look.png" >/dev/null 2>&1
  done
  for kv in "$@"; do unset "SIMCTL_CHILD_${kv%%=*}"; done
  echo "  $picture: $work/$picture-light.png, $work/$picture-dark.png"
}
# The upload form's picture, where the app can read it.
container=$(xcrun simctl get_app_container "$udid" "$bundle" data)
cp "$work/pictures/heart.png" "$container/tmp/Acme-Heart v2.png" 2>/dev/null || { mkdir -p "$container/tmp" && cp "$work/pictures/heart.png" "$container/tmp/Acme-Heart v2.png"; }
if [ "${LUMIA_SMOKE_PICTURES:-}" != none ]; then
  shoot channel channel LUMIA_DEV_CHANNEL=general
  shoot channel-still channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_REDUCE_MOTION=1
  shoot picker channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_SHEET=react
  shoot picker-search channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_SHEET=react LUMIA_DEV_SEARCH=par
  shoot picker-composer channel LUMIA_DEV_CHANNEL=general LUMIA_DEV_SHEET=composer
  shoot list channel LUMIA_DEV_CHANNEL=general "LUMIA_DEV_DRAFT=Shipped the importer :par"
  shoot sections serversettings
  shoot emoji emoji
  shoot emoji-row emoji LUMIA_DEV_EMOJI=parrotwave
  shoot emoji-remove emoji LUMIA_DEV_EMOJI=parrotwave LUMIA_DEV_ASK=remove
  shoot emoji-rename emoji LUMIA_DEV_EMOJI=parrotwave LUMIA_DEV_ASK=rename LUMIA_DEV_EMOJI_NAME=partyparrot
  shoot emoji-upload emoji "LUMIA_DEV_EMOJI_FILE=$container/tmp/Acme-Heart v2.png"
  if [ -n "${LUMIA_SMOKE_DB:-}" ]; then
    psql "$LUMIA_SMOKE_DB" -q -c "update servers set emoji_limit = 12 where id = '$server'" >/dev/null
    shoot emoji-limit emoji
    psql "$LUMIA_SMOKE_DB" -q -c "update servers set emoji_limit = null where id = '$server'" >/dev/null
  else
    echo "  (no LUMIA_SMOKE_DB: no picture of the section at its limit)"
  fi
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
fi
[ "${LUMIA_SMOKE_PICTURES:-}" = only ] && { echo "pictures only: $work"; exit 0; }

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT
run() { # <handle> <test>
  fresh
  xcrun simctl install "$udid" "$app"
  TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$1" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
  TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
    xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
    -only-testing:"LumiaUITests/SmokeUITests/$2" > "$work/$2.log" 2>&1 &
  uitest=$!
  log="$work/$2.log"
}
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    echo "FAIL: the UI test did not reach \"$1\"; see $log"
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$log" | head -10
    exit 1
  fi
  sleep 1
  xcrun simctl io "$udid" screenshot "$work/step-$1.png" >/dev/null 2>&1
}
seen() { touch "$work/steps/$1.seen"; }
passed() {
  wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
  uitest=""
}
message_field() { # <body starts with> <python of m>
  api "$to" GET "/channels/$general/messages?limit=50" | py "m = next((m for m in d.get('messages', []) if m.get('body', '').startswith('''$1''')), None); print('none' if m is None else $2)"
}
reaction_count() { # <message id> <emoji id>
  api "$to" GET "/channels/$general/messages?limit=50" | py "m = next(m for m in d.get('messages', []) if m['messageId'] == '$1'); print(sum(r.get('count', 0) for r in m.get('reactions', []) if r.get('emojiId') == '$2'))"
}

# The photo picker's newest picture: the one Upload emoji chooses.
xcrun simctl addmedia "$udid" "$work/pictures/heart.png"
run "$owner" testEmojiByHand
reached uploaded 240
[ "$(emoji_id acme_heart)" != "" ] || fail "no :acme_heart: on the instance after Upload emoji: $(emoji_names)"
[ "$(emoji_list "print(len(d.get('emoji', [])))")" = 13 ] || fail "Acme has $(emoji_names) after the upload"
seen uploaded
echo "a picture from the photo picker uploaded through Upload emoji, its name made from the file's, a name Acme has refused, then :acme_heart:"
reached reacted 120
[ "$(reaction_count "$fixed" "$(emoji_id partyparrot)")" = 1 ] || fail "the phone's own message has $(reaction_count "$fixed" "$(emoji_id partyparrot)") :partyparrot: reactions"
seen reacted
echo "a reaction with :partyparrot: found by the picker's search, on the instance by the emoji's id"
reached sent 120
[ "$(message_field "Ship it" "len(m.get('emoji', []))")" = 2 ] || fail "the phone's message carries $(message_field "Ship it" "m.get('emoji')") refs, not 2"
[ "$(message_field "Ship it" "m['body']")" = "Ship it :acme_heart: :partyparrot:" ] || fail "the phone's message is \"$(message_field "Ship it" "m['body']")\""
seen sent
echo "a message with :acme_heart: from the composer's emoji button and :partyparrot: from the : list, sent with both refs"
reached listed 120
curl -s -f -X DELETE -H "authorization: Bearer $to" "$instance/api/v1/servers/$server/emoji/$(emoji_id parrotwave)" >/dev/null || fail "could not remove :parrotwave:"
[ "$(emoji_id parrotwave)" = "" ] || fail ":parrotwave: is still listed after its removal"
seen listed
echo "the : list asked again when :parrotwave: was removed while it was open"
reached searched 120
api "$to" PATCH "/servers/$server/emoji/$(emoji_id sadparrot)" '{"name":"gloomy"}' >/dev/null || fail "could not rename :sadparrot:"
[ "$(emoji_id gloomy)" != "" ] || fail "no :gloomy: after the rename: $(emoji_names)"
seen searched
echo "the picker's search asked again when :sadparrot: was renamed :gloomy: while it was open"
reached renamed 120
[ "$(emoji_id acme_love)" != "" ] || fail "no :acme_love: after Rename: $(emoji_names)"
seen renamed
echo "a rename refused for a name Acme has, then :acme_heart: renamed :acme_love:"
reached removed 120
[ "$(emoji_id acme_love)" = "" ] || fail ":acme_love: is still listed after Remove"
[ "$(message_field "Ship it" "m['body']")" = "Ship it :acme_heart: :partyparrot:" ] || fail "the message changed with the removal"
seen removed
passed
echo "Remove asked first, Keep it kept it, and Remove took it; the message keeps its words"
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "ok: $work"
