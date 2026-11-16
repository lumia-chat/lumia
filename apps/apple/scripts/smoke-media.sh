#!/bin/sh
# Attachments and link cards on the phone (specs/media.md), in a simulator
# against an instance built from this tree with a directory blob store (a
# dev or scratch instance). The release CLI seeds a server's channel where
# someone else sends a landscape photo, a PDF and a link whose card the
# instance builds (it fetches the page from the internet, so a machine
# offline gets no card and the script says so); the app is drawn on that
# channel light, dark and in Minimal (Images and Embeds as links). Then
# testAttachingAPictureFromThePhotoLibrary: a picture the script put in
# the simulator's library (`simctl addmedia`, a portrait JPEG) is chosen
# through the attach button and the system's photo picker, uploaded,
# sent with words and drawn; the script reads the message on the instance
# (one IMAGE, upright: taller than wide) and opens the attachment with the
# CLI as the other person; the test opens the viewer (Save to Photos,
# Share, Close) and the PDF's share sheet. Screenshots land in
# .build/smoke-media/.
#
#   scripts/smoke-media.sh     # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4083 LUMIA_SIMULATOR="iPhone 15 Pro Max" LUMIA_CLI=/path/to/lumia-cli scripts/smoke-media.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
link=${LUMIA_SMOKE_LINK:-https://www.rust-lang.org/}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-media}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 5 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 5 "$instance/api/v1/instance" | grep -q uploadLimits || { echo "no instance keeping attachments at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

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
uuid7() { python3 -c "
import os, time, uuid
b = bytearray(int(time.time() * 1000).to_bytes(6, 'big') + os.urandom(10))
b[6] = 0x70 | (b[6] & 0x0f); b[8] = 0x80 | (b[8] & 0x3f)
print(uuid.UUID(bytes=bytes(b)))"; }
fail() { echo "FAIL: $*"; exit 1; }

stamp=$(date +%s)
handle="sam$stamp"
other="priya$stamp"
person mine "$handle" "Sam Rivera"
server=$(c mine server-create "Media smoke" | uuid)
c mine channel-create "$server" general >/dev/null
channel=$(c mine channels "$server" | uuid)
invite=$(c mine invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
person other "$other" "Priya Raman" --invite "$invite"

# The files: a landscape photo and a PDF sent by the other person, and a
# portrait picture for the phone's library.
python3 "$here/media.py" picture "$work/ridge.png" 1200 800
sips -s format jpeg "$work/ridge.png" --out "$work/ridge.jpg" >/dev/null
python3 "$here/media.py" picture "$work/lake.png" 900 1200
sips -s format jpeg "$work/lake.png" --out "$work/lake.jpg" >/dev/null
python3 "$here/media.py" pdf "$work/notes.pdf" "Notes from Thursday"
c mine send "$channel" "Morning! Anything from the trip?" >/dev/null
c other send "$channel" "The view from the ridge this morning." --attach "$work/ridge.jpg" >/dev/null
c other send "$channel" "Notes from Thursday, as promised." --attach "$work/notes.pdf" >/dev/null
# The card: asked for over HTTP (the CLI names no cards), built by the instance after the send.
theirs=$(token "$other")
id=$(uuid7)
api "$theirs" POST "/channels/$channel/commands" "{\"commandId\":\"$id\",\"channelId\":\"$channel\",\"sendMessage\":{\"body\":\"Release notes are up: $link\",\"cardUrls\":[\"$link\"]}}" >/dev/null
carded=""
for _ in $(seq 1 20); do
  carded=$(api "$theirs" GET "/channels/$channel/messages?limit=10" | py "print(sum(1 for m in d.get('messages', []) if m.get('cards')))")
  [ "$carded" -gt 0 ] && break
  sleep 1
done
[ "$carded" -gt 0 ] && echo "the instance built a card for $link" || echo "note: no card for $link (the instance could not fetch it); the pictures will have none"
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
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
xcrun simctl install "$udid" "$app"
xcrun simctl addmedia "$udid" "$work/lake.jpg"

shoot() { # <name> <seconds> [<appearance> [<preset>]]
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$handle" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" SIMCTL_CHILD_LUMIA_DEV_SCREEN=channel SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 \
  SIMCTL_CHILD_LUMIA_DEV_APPEARANCE="${3:-}" SIMCTL_CHILD_LUMIA_DEV_PRESET="${4:-}" \
    xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep "$2"
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
  echo "  $1: $work/$1.png"
}
shoot channel 12
shoot channel-dark 8 dark
shoot channel-minimal 8 "" minimal
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$handle" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_SCREEN=channel TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" TEST_RUNNER_LUMIA_UITEST_CARD="$carded" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testAttachingAPictureFromThePhotoLibrary > "$work/uitest.log" 2>&1 &
uitest=$!

# Waits for the test to announce a step; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && return 0
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  [ -f "$work/steps/$1" ] && return 0
  xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
  echo "FAIL: the UI test did not reach \"$1\"; see $work/uitest.log"
  grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10
  exit 1
}
seen() { xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1 || true; echo > "$work/steps/$1.seen"; }

reached drawn 240
seen drawn
echo "the channel draws the photo, the file and the card as elements of their own"
reached picker 60
seen picker
reached strip 90
seen strip
reached sent 90
seen sent
# The phone's message on the instance: its words and one picture, upright.
mine=$(token "$handle")
got=$(api "$mine" GET "/channels/$channel/messages?limit=10" | py "
m = [x for x in d.get('messages', []) if x.get('body') == 'From the phone.'][0]
a = m.get('attachments', [])
print(m['messageId'], len(a), a[0].get('kind') if a else '', a[0].get('width', 0) if a else 0, a[0].get('height', 0) if a else 0)")
set -- $got
[ "$2" = 1 ] || fail "the phone's message carries $2 attachments, not one: $got"
case "$3" in *IMAGE*) ;; *) fail "the phone's attachment is not an image: $got" ;; esac
[ "$5" -gt "$4" ] || fail "the phone's picture is not upright (portrait): $got"
c other attachments "$channel" "$1" --out "$work/received" >/dev/null
opened=$(ls "$work/received" | head -1)
[ -n "$opened" ] || fail "the other person could not open the phone's attachment"
echo "the phone sent one picture, $4×$5, opened by the other person as $opened ($(sips -g pixelWidth -g pixelHeight "$work/received/$opened" | awk '/pixel/ {print $2}' | paste -sd x -))"
reached viewer 60
seen viewer
reached share 60
seen share
wait "$uitest" || { echo "FAIL: the UI test did not pass; see $work/uitest.log"; grep -E "error|failed|XCTAssert" "$work/uitest.log" | head -10; exit 1; }
uitest=""
# The channel again, landing at its end: the phone's own picture drawn from its thumbnail.
shoot after 12
echo "smoke-media: ok (a photo, a file and a card drawn; a picture from the library attached, sent, drawn and opened by the other side; the viewer and the share sheet opened; screenshots under $work)"
